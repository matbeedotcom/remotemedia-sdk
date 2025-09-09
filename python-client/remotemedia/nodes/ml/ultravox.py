import asyncio
import numpy as np
import logging
import hashlib
import json
from typing import Optional, Any, AsyncGenerator, Dict, List, Callable
from datetime import datetime, timedelta

from remotemedia.core.node import Node
from remotemedia.core.exceptions import NodeError

# Configure basic logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

try:
    import torch
    from transformers import pipeline
except ImportError:
    logger.warning("ML libraries not found. UltravoxNode will not be available.")
    torch = None
    pipeline = None


class UltravoxNode(Node):
    """
    A node that uses the Ultravox model for multimodal audio/text to text generation.
    It receives audio chunks and immediately processes them to generate a text response.
    
    Supports tool calling for extending the model's capabilities. Example usage:
    
    ```python
    # Define tools
    tools = [
        {
            "name": "get_weather",
            "description": "Get current weather for a location",
            "parameters": {
                "properties": {
                    "location": {"type": "string", "description": "City name"},
                    "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
                }
            }
        }
    ]
    
    # Define tool executors
    async def get_weather(location: str, unit: str = "celsius"):
        # Implementation here
        return f"25°{unit[0].upper()} and sunny in {location}"
    
    tool_executors = {
        "get_weather": get_weather
    }
    
    # Create node with tools
    node = UltravoxNode(
        tools=tools,
        tool_executors=tool_executors
    )
    ```
    """

    def __init__(self,
                 model_id: str = "fixie-ai/ultravox-v0_5-llama-3_2-1b",
                 device: Optional[str] = None,
                 torch_dtype: str = "bfloat16",
                 max_new_tokens: int = 16382,
                 system_prompt: str = "You are a friendly and helpful AI assistant.",
                 enable_conversation_history: bool = True,
                 conversation_history_minutes: float = 10.0,
                 tools: Optional[List[Dict[str, Any]]] = None,
                 tool_executors: Optional[Dict[str, Callable]] = None,
                 **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self.is_streaming = True
        self.model_id = model_id
        self._requested_device = device
        self._requested_torch_dtype = torch_dtype
        self.sample_rate = 16000  # Ultravox expects 16kHz audio
        self.max_new_tokens = max_new_tokens
        self.system_prompt = system_prompt
        self.enable_conversation_history = enable_conversation_history
        self.conversation_history_minutes = conversation_history_minutes
        self.tools = tools or []
        self.tool_executors = tool_executors or {}
        
        # Build tool schemas and enhance system prompt if tools are provided
        if self.tools:
            self._build_tool_system_prompt()
            # Add tool usage instructions to system prompt
            tool_names = [tool['name'] for tool in self.tools]
            self.system_prompt += f"\n\nYou have access to the following tools: {', '.join(tool_names)}. Use them when appropriate to help answer user questions."

        self.llm_pipeline = None
        self.device = None
        self.torch_dtype = None

    def _build_tool_system_prompt(self) -> None:
        """Build tool schemas for the transformers pipeline."""
        # Convert tool definitions to the format expected by transformers
        # The tools should be in the format expected by the chat template
        self.tool_schemas = []
        for tool in self.tools:
            # Transform to the expected schema format
            tool_schema = {
                "type": "function",
                "function": {
                    "name": tool['name'],
                    "description": tool['description'],
                    "parameters": {
                        "type": "object",
                        "properties": tool.get('parameters', {}).get('properties', {}),
                        "required": tool.get('parameters', {}).get('required', [])
                    }
                }
            }
            self.tool_schemas.append(tool_schema)
    
    async def initialize(self) -> None:
        """
        Load the model and processor. This runs on the execution environment (local or remote).
        """
        await super().initialize()
        
        # Only initialize if not already done
        if self.llm_pipeline is not None:
            logger.info("UltravoxNode already initialized, skipping.")
            return
            
        try:
            import torch
            from transformers import pipeline
        except ImportError:
            raise NodeError("Required ML libraries (torch, transformers, peft) are not installed on the execution environment.")

        if self._requested_device:
            self.device = self._requested_device
        elif torch.cuda.is_available():
            self.device = "cuda:0"
        elif hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            self.device = "mps"
            if self._requested_torch_dtype == "bfloat16":
                self.torch_dtype = torch.bfloat16
        else:
            self.device = "cpu"

        if not self.torch_dtype:
            try:
                resolved_torch_dtype = getattr(torch, self._requested_torch_dtype)
            except AttributeError:
                raise NodeError(f"Invalid torch_dtype '{self._requested_torch_dtype}'")
            self.torch_dtype = resolved_torch_dtype if torch.cuda.is_available() else torch.float32
        
        logger.info(f"UltravoxNode configured for model '{self.model_id}' on device '{self.device}'")
        logger.info(f"Initializing Ultravox model '{self.model_id}'...")
        try:
            self.llm_pipeline = await asyncio.to_thread(
                pipeline,
                model=self.model_id,
                torch_dtype=self.torch_dtype,
                device=self.device,
                trust_remote_code=True
            )
            logger.info("Ultravox model initialized successfully.")
        except Exception as e:
            raise NodeError(f"Failed to initialize Ultravox model: {e}")

    async def _generate_response(self, audio_data: np.ndarray, session_id: Optional[str] = None, user_text: Optional[str] = None) -> Optional[str]:
        """Run model inference in a separate thread with conversation context."""
        logger.info(f"Generating response for {len(audio_data) / self.sample_rate:.2f}s of audio...")
        
        # Build conversation turns
        turns = [{"role": "system", "content": self.system_prompt}]
        
        # Get conversation history from state if enabled
        logger.info(f"Generating response for session ID: {session_id}, enable_conversation_history: {self.enable_conversation_history}")
        if self.enable_conversation_history and session_id:
            session_state = await self.get_session_state(session_id)
            if session_state:
                history = session_state.get('conversation_history', [])
                current_time = datetime.now()
                cutoff_time = current_time - timedelta(minutes=self.conversation_history_minutes)
                
                # Add previous conversation turns, filtering by time and formatting for model
                for msg in history:
                    if msg.get("role") != "system":  # Skip system messages as we already added it
                        # Check timestamp if present
                        if 'timestamp' in msg:
                            msg_time = datetime.fromisoformat(msg['timestamp'])
                            if msg_time >= cutoff_time:
                                # Add turn without audio - previous audio is already tokenized in content
                                turns.append({
                                    "role": msg["role"],
                                    "content": msg["content"]
                                })
                        else:
                            # Legacy message without timestamp - skip it
                            pass
                
                logger.info(f"UltravoxNode: Using {len(turns)} turns in conversation context for session {session_id} "
                            f"(from last {self.conversation_history_minutes} minutes)")
                # Debug: Log the actual conversation turns
                logger.info("=== CONVERSATION HISTORY DEBUG ===")
                for i, turn in enumerate(turns):
                    content = turn.get('content', '')
                    # Show more content for debugging
                    content_preview = content[:200] + "..." if len(content) > 200 else content
                    logger.info(f"Turn {i}: role='{turn.get('role')}', content='{content_preview}'")
        
        try:
            # Build input for model using the Ultravox pipeline format
            # The pipeline expects:
            # - turns: conversation history
            # - audio: current audio numpy array
            # - sampling_rate: audio sample rate
            
            # If user provided text, add it to the current turn
            if user_text:
                # Add a new user turn with the text
                turns.append({
                    "role": "user",
                    "content": user_text
                })
            # The pipeline will automatically handle the audio and add <|audio|> token
            
            input_data = {
                'turns': turns,
                'audio': audio_data,
                'sampling_rate': self.sample_rate
            }
            
            # Debug: Log what we're sending to the model
            logger.info("=== SENDING TO PIPELINE ===")
            logger.info(f"Audio shape: {audio_data.shape if audio_data is not None else None}")
            logger.info(f"Audio dtype: {audio_data.dtype if audio_data is not None else None}")
            logger.info(f"Audio range: [{audio_data.min():.6f}, {audio_data.max():.6f}]" if audio_data is not None else "No audio")
            logger.info(f"Turns count: {len(turns)}")
            logger.info(f"Input data keys: {list(input_data.keys())}")
            
            # Log the exact input structure
            logger.info("Full input_data structure:")
            for key, value in input_data.items():
                if key == 'audio':
                    logger.info(f"  {key}: ndarray shape={value.shape if hasattr(value, 'shape') else 'N/A'}")
                elif key == 'turns':
                    logger.info(f"  {key}: {len(value)} turns")
                    for idx, turn in enumerate(value[-3:]):  # Show last 3 turns
                        logger.info(f"    Recent turn {idx}: {turn}")
                else:
                    logger.info(f"  {key}: {value}")
            
            # Try to preview what the chat template will produce
            try:
                if hasattr(self.llm_pipeline, 'tokenizer') and hasattr(self.llm_pipeline.tokenizer, 'apply_chat_template'):
                    # If we have tools, pass them to the chat template
                    tools_arg = self.tool_schemas if self.tools else None
                    preview_text = self.llm_pipeline.tokenizer.apply_chat_template(
                        turns, 
                        tools=tools_arg,
                        add_generation_prompt=True, 
                        tokenize=False
                    )
                    logger.info("=== CHAT TEMPLATE PREVIEW ===")
                    logger.info(f"Template output (first 1000 chars): {preview_text[:1000]}...")
                    if len(preview_text) > 1000:
                        logger.info(f"... (truncated, total length: {len(preview_text)})")
                    # Log if tools were included
                    if tools_arg:
                        logger.info(f"Tools included in template: {[t['function']['name'] for t in tools_arg]}")
            except Exception as e:
                logger.debug(f"Could not preview chat template: {e}")
            
            # Call the pipeline with the correct format
            # Include tools if available
            pipeline_kwargs = {
                "max_new_tokens": self.max_new_tokens
            }
            
            # Add tools to the pipeline call if available
            if self.tools and hasattr(self, 'tool_schemas'):
                pipeline_kwargs["tools"] = self.tool_schemas
                logger.info(f"Passing {len(self.tool_schemas)} tools to pipeline")
            
            result = await asyncio.to_thread(
                self.llm_pipeline,
                input_data,
                **pipeline_kwargs
            )
            logger.info("=== PIPELINE RESULT ===")
            logger.info(f"Result type: {type(result)}")
            if isinstance(result, str):
                logger.info(f"Result (first 500 chars): {result[:500]}...")
            else:
                logger.info(f"Result: {result}")
            
            # Extract response
            # The Ultravox pipeline postprocesses to return just the generated text
            if isinstance(result, str):
                response = result
            elif isinstance(result, list) and result and isinstance(result[0], dict) and 'generated_text' in result[0]:
                response = result[0]['generated_text']
            else:
                logger.warning(f"Model did not return an expected response format. Full result: {result}")
                return None

            response = response.strip()
            logger.info(f"Ultravox generated response: '{response}'")
            
            # Check if the response contains a tool call
            tool_result = None
            tool_call = None
            if self.tools and response:
                logger.info(f"Checking response for tool calls. Response: '{response[:200]}...'")
                tool_call, remaining_text = self._parse_tool_call(response)
                if tool_call:
                    logger.info(f"Detected tool call: {tool_call}")
                    # Execute the tool
                    tool_result = await self._execute_tool(tool_call)
                else:
                    logger.info("No tool call detected in response")
                    
                    # If there's a tool result, we need to generate a new response with the result
                    if tool_result is not None:
                        # Add tool call and result to conversation using native format
                        tool_turn = {
                            "role": "assistant",
                            "tool_calls": [{
                                "type": "function",
                                "function": tool_call
                            }]
                        }
                        result_turn = {
                            "role": "tool",
                            "name": tool_call.get('name'),
                            "content": str(tool_result)
                        }
                        
                        # Generate a new response with the tool result
                        turns_with_tool = turns + [tool_turn, result_turn]
                        logger.info(f"Generating response with tool result: {tool_result}")
                        
                        # Make another call to the pipeline with the tool result
                        followup_input = {
                            'turns': turns_with_tool,
                            'audio': np.array([]),  # Empty audio for follow-up
                            'sampling_rate': self.sample_rate
                        }
                        
                        # Include tools in follow-up call as well
                        followup_kwargs = {
                            "max_new_tokens": self.max_new_tokens
                        }
                        if self.tools and hasattr(self, 'tool_schemas'):
                            followup_kwargs["tools"] = self.tool_schemas
                        
                        followup_result = await asyncio.to_thread(
                            self.llm_pipeline,
                            followup_input,
                            **followup_kwargs
                        )
                        
                        if isinstance(followup_result, str):
                            response = followup_result
                        elif isinstance(followup_result, list) and followup_result:
                            response = followup_result[0].get('generated_text', response)
                        
                        logger.info(f"Final response after tool call: {response}")
                    elif remaining_text:
                        # If no tool executor, use any remaining text as response
                        response = remaining_text
            
            # Add the interaction to history
            if self.enable_conversation_history and response and session_id:
                session_state = await self.get_session_state(session_id)
                if session_state:
                    history = session_state.get('conversation_history', [])
                    current_time = datetime.now()
                    
                    # Add new interaction with timestamps
                    # Note: We don't store <|audio|> in history to avoid multiple audio placeholders
                    # The pipeline only expects one <|audio|> token for the current audio
                    user_content = user_text if user_text else f"[Audio input: {len(audio_data) / self.sample_rate:.1f}s]"
                    history.append({
                        "role": "user", 
                        "content": user_content,
                        "timestamp": current_time.isoformat()
                    })
                    
                    # If there was a tool call, add it to history
                    if tool_result is not None:
                        history.append({
                            "role": "assistant",
                            "tool_calls": [{
                                "type": "function",
                                "function": tool_call
                            }],
                            "timestamp": current_time.isoformat()
                        })
                        history.append({
                            "role": "tool",
                            "name": tool_call.get('name'),
                            "content": str(tool_result),
                            "timestamp": current_time.isoformat()
                        })
                    
                    history.append({
                        "role": "assistant", 
                        "content": response,
                        "timestamp": current_time.isoformat()
                    })
                    
                    # Filter history to keep only recent messages within time window
                    cutoff_time = current_time - timedelta(minutes=self.conversation_history_minutes)
                    filtered_history = []
                    for msg in history:
                        # Handle messages with timestamps
                        if isinstance(msg, dict) and 'timestamp' in msg:
                            msg_time = datetime.fromisoformat(msg['timestamp'])
                            if msg_time >= cutoff_time:
                                filtered_history.append(msg)
                        else:
                            # Keep messages without timestamps (legacy) but don't include in turns
                            # This ensures backward compatibility
                            pass
                    
                    # Update state with filtered history
                    session_state.set('conversation_history', filtered_history)
                    logger.debug(f"UltravoxNode: Updated conversation history for session {session_id} "
                                f"(kept {len(filtered_history)} messages from last {self.conversation_history_minutes} minutes)")
            
            return response
        except Exception as e:
            logger.error(f"Error during Ultravox inference: {e}", exc_info=True)
            return None

    def _parse_tool_call(self, response: str) -> tuple[Optional[Dict[str, Any]], Optional[str]]:
        """
        Parse a tool call from the model's response.
        Returns (tool_call_dict, remaining_text) or (None, original_response).
        
        Handles multiple formats:
        1. Native transformers format with <tool_call> tags
        2. Function call markers used by some models
        3. JSON format
        """
        # First check for native tool call markers used by transformers
        if "<tool_call>" in response and "</tool_call>" in response:
            try:
                # Extract content between tool_call tags
                start = response.find("<tool_call>") + len("<tool_call>")
                end = response.find("</tool_call>")
                tool_json = response[start:end].strip()
                
                # Parse the JSON
                tool_data = json.loads(tool_json)
                
                # Extract remaining text after tool call
                remaining_text = response[end + len("</tool_call>"):].strip()
                
                # Normalize to our expected format
                if isinstance(tool_data, dict):
                    # Handle different possible formats
                    if 'function' in tool_data:
                        return tool_data['function'], remaining_text
                    elif 'name' in tool_data and 'arguments' in tool_data:
                        return tool_data, remaining_text
                        
            except (json.JSONDecodeError, Exception) as e:
                logger.debug(f"Failed to parse native tool call format: {e}")
        
        # Check for function call format used by some models
        # Format: functionName(arg1=value1, arg2=value2)
        import re
        func_pattern = r'(\w+)\((.*?)\)'
        match = re.match(func_pattern, response.strip())
        if match:
            func_name = match.group(1)
            args_str = match.group(2)
            
            # Check if this matches one of our tools
            if any(tool['name'] == func_name for tool in self.tools):
                try:
                    # Parse arguments
                    args = {}
                    if args_str:
                        # Simple argument parsing (handles key=value pairs)
                        arg_pattern = r'(\w+)\s*=\s*["\']?([^"\']*)["\']?'
                        for arg_match in re.finditer(arg_pattern, args_str):
                            key = arg_match.group(1)
                            value = arg_match.group(2)
                            # Try to parse as JSON for proper types
                            try:
                                args[key] = json.loads(value)
                            except:
                                args[key] = value
                    
                    tool_call = {
                        'name': func_name,
                        'arguments': args
                    }
                    remaining_text = response[match.end():].strip()
                    logger.info(f"Parsed function call format: {tool_call}")
                    return tool_call, remaining_text
                except Exception as e:
                    logger.debug(f"Failed to parse function call format: {e}")
        
        # Fallback to JSON parsing
        try:
            # Try to find JSON in the response
            json_start = response.find('{')
            json_end = response.rfind('}') + 1
            
            if json_start >= 0 and json_end > json_start:
                json_str = response[json_start:json_end]
                parsed = json.loads(json_str)
                
                # Check if it has the expected tool call structure
                if 'tool_call' in parsed and isinstance(parsed['tool_call'], dict):
                    tool_call = parsed['tool_call']
                    if 'name' in tool_call and 'arguments' in tool_call:
                        # Extract any text after the JSON
                        remaining_text = response[json_end:].strip()
                        return tool_call, remaining_text
                
                # Alternative format: direct tool call
                elif 'name' in parsed and 'arguments' in parsed:
                    remaining_text = response[json_end:].strip()
                    return parsed, remaining_text
                    
        except json.JSONDecodeError:
            pass
        
        return None, response
    
    async def _execute_tool(self, tool_call: Dict[str, Any]) -> Any:
        """
        Execute a tool call and return the result.
        """
        tool_name = tool_call.get('name')
        tool_args = tool_call.get('arguments', {})
        
        if tool_name not in self.tool_executors:
            logger.warning(f"Tool '{tool_name}' not found in tool executors")
            return f"Error: Tool '{tool_name}' is not available"
        
        try:
            executor = self.tool_executors[tool_name]
            
            # Check if executor is async
            if asyncio.iscoroutinefunction(executor):
                result = await executor(**tool_args)
            else:
                result = await asyncio.to_thread(executor, **tool_args)
            
            logger.info(f"Tool '{tool_name}' executed successfully with result: {result}")
            return result
            
        except Exception as e:
            logger.error(f"Error executing tool '{tool_name}': {e}", exc_info=True)
            return f"Error executing tool '{tool_name}': {str(e)}"
    
    async def process(self, data_stream: AsyncGenerator[Any, None]) -> AsyncGenerator[Any, None]:
        """
        Process an incoming audio stream and yield generated text responses.
        Expects tuples of (numpy_array, sample_rate) or (numpy_array, sample_rate, metadata_dict).
        
        The node uses the built-in state management system to maintain conversation history
        per session. Session IDs can be provided in the metadata or extracted automatically.
        """
        if not self.llm_pipeline:
            raise NodeError("Ultravox pipeline is not initialized.")

        async for data in data_stream:
            logger.info(f"Processing data: {data}")
            audio_chunk = None
            sample_rate = None
            metadata = {}
            
            # Extract session ID from data
            session_id = self.extract_session_id(data)
            logger.info(f"Session ID: {session_id}")
            
            # Handle different input formats
            if isinstance(data, tuple):
                if len(data) == 2:
                    audio_chunk, sample_rate = data
                elif len(data) == 3:
                    audio_chunk, sample_rate, metadata = data
                else:
                    logger.warning(f"UltravoxNode received tuple of unexpected length {len(data)}, skipping.")
                    continue
            else:
                logger.warning(f"UltravoxNode received data of unexpected type {type(data)}, skipping.")
                continue

            if not isinstance(audio_chunk, np.ndarray):
                logger.warning(f"Received non-numpy audio_chunk of type {type(audio_chunk)}, skipping.")
                continue
            
            # Process metadata
            user_text = None
            if metadata:
                # Clear history if requested
                if metadata.get('clear_history', False) and session_id:
                    session_state = await self.get_session_state(session_id)
                    if session_state:
                        session_state.set('conversation_history', [])
                        logger.info(f"UltravoxNode: Cleared conversation history for session {session_id}")
                
                # Get any user text
                user_text = metadata.get('user_text')

            # Process the audio chunk
            audio_data = audio_chunk.flatten().astype(np.float32)
            if len(audio_data) > 0:
                response = await self._generate_response(audio_data, session_id, user_text)
                if response:
                    # Include state info in output if we have a session
                    if self.enable_conversation_history and session_id:
                        session_state = await self.get_session_state(session_id)
                        if session_state:
                            output_data = {
                                'response': response,
                                'session_id': session_id,
                                'conversation_length': len(session_state.get('conversation_history', []))
                            }
                            yield (response, output_data)
                        else:
                            yield (response,)
                    else:
                        yield (response,)

    async def flush(self) -> Optional[tuple]:
        """No buffering needed - flush is a no-op."""
        return None

    async def cleanup(self) -> None:
        """Clean up the model resources."""
        # Call parent cleanup which handles state cleanup
        await super().cleanup()
        
        # Clear model resources
        self.llm_pipeline = None
        self.device = None
        self.torch_dtype = None
        
        logger.info("UltravoxNode cleaned up.")


__all__ = ["UltravoxNode"] 