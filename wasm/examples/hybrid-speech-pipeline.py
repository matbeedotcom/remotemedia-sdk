#!/usr/bin/env python3
"""
Hybrid Edge-Cloud Speech Pipeline with WasmEdge Integration

This example demonstrates how to enhance your existing VAD + Ultravox + Kokoro pipeline
with intelligent edge processing using WASM modules that can communicate with remote
services via gRPC.

Edge Processing (WASM):
- Fast Voice Activity Detection
- Audio preprocessing and noise reduction  
- Simple audio enhancements
- Lightweight TTS for simple responses

Cloud Processing (gRPC):
- Complex ML models (Ultravox ASR)
- High-quality TTS (Kokoro)
- Resource-intensive operations

The pipeline intelligently routes between edge and cloud based on:
- Data characteristics (size, complexity)
- Performance metrics (latency, success rates)
- Resource availability
"""

import asyncio
import logging
import numpy as np
import os
from pathlib import Path
from typing import AsyncGenerator, Any, Tuple

# Add parent directory to path for imports
import sys
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from remotemedia.core.pipeline import Pipeline
from remotemedia.core.node import RemoteExecutorConfig
from remotemedia.nodes.source import MediaReaderNode, AudioTrackSource
from remotemedia.nodes.audio import AudioTransform
from remotemedia.nodes.ml import UltravoxNode, KokoroTTSNode
from remotemedia.nodes.remote import RemoteObjectExecutionNode

# Import WASM integration components
from wasm.src.python.nodes.wasm_node import WasmEdgeNode, WasmConfig
from wasm.src.python.nodes.hybrid_wasm_node import HybridWasmNode
from wasm.src.python.routing.intelligent_router import IntelligentRouter, ProcessingTarget

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


class HybridSpeechPipeline:
    """
    Enhanced speech-to-speech pipeline with intelligent edge-cloud routing.
    """
    
    def __init__(self, remote_host: str = "localhost", remote_port: int = 50052):
        """
        Initialize the hybrid speech pipeline.
        
        Args:
            remote_host: gRPC server host
            remote_port: gRPC server port
        """
        self.remote_config = RemoteExecutorConfig(
            host=remote_host,
            port=remote_port,
            ssl_enabled=False
        )
        
        self.pipeline = None
        self.stats = {
            "edge_processed": 0,
            "cloud_processed": 0,
            "total_latency_ms": 0,
            "edge_latency_ms": 0,
            "cloud_latency_ms": 0
        }
    
    async def create_pipeline(self) -> Pipeline:
        """Create the hybrid edge-cloud pipeline."""
        
        pipeline = Pipeline(name="HybridSpeechPipeline")
        
        # ========== Stage 1: Audio Input ==========
        pipeline.add_node(MediaReaderNode(
            path="examples/audio.wav",
            chunk_size=4096,
            name="MediaReader"
        ))
        
        pipeline.add_node(AudioTrackSource(name="AudioTrackSource"))
        
        # Resample to 16kHz for processing
        pipeline.add_node(AudioTransform(
            output_sample_rate=16000,
            output_channels=1,
            name="AudioResampler"
        ))
        
        # ========== Stage 2: Voice Activity Detection (Hybrid) ==========
        
        # Edge VAD - Fast WASM implementation
        edge_vad = WasmEdgeNode(
            wasm_config=WasmConfig(
                wasm_path="modules/audio/fast_vad.wasm",
                function_name="detect_speech",
                memory_limit=32 * 1024 * 1024,  # 32MB
                timeout_ms=100  # 100ms max
            ),
            enable_streaming=True,
            name="EdgeVAD"
        )
        
        # Cloud VAD - High accuracy remote service
        cloud_vad = RemoteObjectExecutionNode(
            obj_to_execute=VoiceActivityDetector(
                frame_duration_ms=30,
                energy_threshold=0.02,
                speech_threshold=0.3
            ),
            remote_config=self.remote_config,
            name="CloudVAD"
        )
        
        # Intelligent VAD router
        def vad_routing_decision(data: Any) -> ProcessingTarget:
            """Route VAD based on audio characteristics."""
            audio_data, metadata = pipeline.split_data_metadata(data)
            
            # Fast path for clear audio
            if hasattr(audio_data, 'shape'):
                # Calculate basic energy
                energy = np.mean(np.abs(audio_data))
                
                # Clear speech or silence - use edge
                if energy < 0.01 or energy > 0.1:
                    return ProcessingTarget.EDGE
                
                # Uncertain - use cloud for accuracy
                return ProcessingTarget.CLOUD
            
            return ProcessingTarget.AUTO
        
        vad_router = IntelligentRouter(
            edge_node=edge_vad,
            cloud_node=cloud_vad,
            decision_function=vad_routing_decision,
            edge_timeout_ms=100,
            cloud_timeout_ms=1000,
            name="VADRouter"
        )
        
        pipeline.add_node(vad_router)
        
        # ========== Stage 3: Audio Preprocessing (Edge) ==========
        
        # WASM audio preprocessing - always on edge for low latency
        audio_preprocessor = HybridWasmNode(
            wasm_config=WasmConfig(
                wasm_path="modules/audio/audio_preprocess.wasm",
                function_name="preprocess_audio",
                memory_limit=64 * 1024 * 1024  # 64MB
            ),
            remote_config=self.remote_config,
            available_services={
                "AudioTransform": "remotemedia.nodes.audio.AudioTransform",
            },
            name="EdgePreprocessor"
        )
        
        pipeline.add_node(audio_preprocessor)
        
        # ========== Stage 4: Speech Recognition (Hybrid) ==========
        
        # Edge ASR - Simple keyword recognition
        edge_asr = WasmEdgeNode(
            wasm_config=WasmConfig(
                wasm_path="modules/audio/simple_asr.wasm",
                function_name="transcribe",
                memory_limit=256 * 1024 * 1024,  # 256MB
                timeout_ms=5000  # 5 seconds
            ),
            name="EdgeASR"
        )
        
        # Cloud ASR - Full Ultravox model
        cloud_asr = RemoteObjectExecutionNode(
            obj_to_execute=UltravoxNode(
                model_id="fixie-ai/ultravox-v0_5-llama-3_1-8b",
                system_prompt=(
                    "You are a helpful assistant. Listen to what the user says and respond "
                    "appropriately and concisely. Keep responses under 2 sentences."
                ),
                buffer_duration_s=10.0
            ),
            remote_config=self.remote_config,
            name="CloudUltravox"
        )
        
        # Intelligent ASR routing
        def asr_routing_decision(data: Any) -> ProcessingTarget:
            """Route ASR based on audio duration and complexity."""
            audio_data, metadata = pipeline.split_data_metadata(data)
            
            if hasattr(audio_data, 'shape'):
                duration = audio_data.shape[-1] / 16000  # Assuming 16kHz
                
                # Short utterances to edge (< 3 seconds)
                if duration < 3.0:
                    return ProcessingTarget.EDGE
                
                # Long or complex audio to cloud
                return ProcessingTarget.CLOUD
            
            return ProcessingTarget.AUTO
        
        asr_router = IntelligentRouter(
            edge_node=edge_asr,
            cloud_node=cloud_asr,
            decision_function=asr_routing_decision,
            edge_timeout_ms=5000,
            cloud_timeout_ms=30000,
            name="ASRRouter"
        )
        
        pipeline.add_node(asr_router)
        
        # ========== Stage 5: Text-to-Speech (Hybrid) ==========
        
        # Edge TTS - Fast, simple synthesis
        edge_tts = WasmEdgeNode(
            wasm_config=WasmConfig(
                wasm_path="modules/audio/simple_tts.wasm",
                function_name="synthesize",
                memory_limit=128 * 1024 * 1024,  # 128MB
                timeout_ms=5000
            ),
            enable_streaming=True,
            name="EdgeTTS"
        )
        
        # Cloud TTS - High-quality Kokoro
        cloud_tts = RemoteObjectExecutionNode(
            obj_to_execute=KokoroTTSNode(
                lang_code='a',
                voice='af_heart',
                speed=1.0,
                sample_rate=24000,
                stream_chunks=True
            ),
            remote_config=self.remote_config,
            name="CloudKokoro"
        )
        
        # Intelligent TTS routing
        def tts_routing_decision(data: Any) -> ProcessingTarget:
            """Route TTS based on text complexity."""
            text = str(data[0]) if isinstance(data, tuple) else str(data)
            
            # Simple responses to edge
            if len(text.split()) <= 8 and not any(char in text for char in '.,!?;:'):
                logger.info(f"Routing simple text to edge TTS: '{text[:50]}'")
                return ProcessingTarget.EDGE
            
            # Complex or expressive text to cloud
            logger.info(f"Routing complex text to cloud TTS: '{text[:50]}'")
            return ProcessingTarget.CLOUD
        
        tts_router = IntelligentRouter(
            edge_node=edge_tts,
            cloud_node=cloud_tts,
            decision_function=tts_routing_decision,
            edge_timeout_ms=5000,
            cloud_timeout_ms=30000,
            collect_metrics=True,
            name="TTSRouter"
        )
        
        pipeline.add_node(tts_router)
        
        # ========== Stage 6: Audio Enhancement (Edge) ==========
        
        # Final audio enhancement - always on edge for low latency
        audio_enhancer = WasmEdgeNode(
            wasm_config=WasmConfig(
                wasm_path="modules/audio/audio_enhance.wasm",
                function_name="enhance_audio",
                memory_limit=64 * 1024 * 1024  # 64MB
            ),
            name="EdgeEnhancer"
        )
        
        pipeline.add_node(audio_enhancer)
        
        self.pipeline = pipeline
        return pipeline
    
    async def run(self) -> None:
        """Run the hybrid speech pipeline."""
        
        logger.info("🚀 Starting Hybrid Edge-Cloud Speech Pipeline")
        logger.info("=" * 60)
        logger.info("Pipeline Configuration:")
        logger.info("  - Edge: WASM modules for fast, simple processing")
        logger.info("  - Cloud: gRPC services for complex ML models")
        logger.info("  - Routing: Intelligent decisions based on data characteristics")
        logger.info("=" * 60)
        
        # Create and initialize pipeline
        pipeline = await self.create_pipeline()
        
        # Track processing statistics
        start_time = asyncio.get_event_loop().time()
        chunk_count = 0
        
        async with pipeline.managed_execution():
            try:
                async for result in pipeline.process():
                    chunk_count += 1
                    
                    # Log processing progress
                    if chunk_count % 10 == 0:
                        elapsed = asyncio.get_event_loop().time() - start_time
                        logger.info(f"Processed {chunk_count} chunks in {elapsed:.2f}s")
                    
                    # Collect routing statistics from routers
                    for node in pipeline.nodes:
                        if isinstance(node, IntelligentRouter):
                            stats = node.get_routing_stats()
                            logger.debug(f"{node.name} stats: {stats}")
                
            except Exception as e:
                logger.error(f"Pipeline error: {e}", exc_info=True)
                raise
        
        # Print final statistics
        elapsed = asyncio.get_event_loop().time() - start_time
        logger.info("=" * 60)
        logger.info(f"✅ Pipeline completed in {elapsed:.2f}s")
        logger.info(f"   Processed {chunk_count} chunks")
        
        # Collect and display routing statistics
        for node in pipeline.nodes:
            if isinstance(node, IntelligentRouter):
                stats = node.get_routing_stats()
                logger.info(f"\n{node.name} Statistics:")
                logger.info(f"  Edge requests: {stats['edge_requests']}")
                logger.info(f"  Cloud requests: {stats['cloud_requests']}")
                logger.info(f"  Edge latency: {stats['edge_latency_ms']:.2f}ms")
                logger.info(f"  Cloud latency: {stats['cloud_latency_ms']:.2f}ms")
                logger.info(f"  Edge success rate: {stats['edge_success_rate']*100:.1f}%")
                logger.info(f"  Cloud success rate: {stats['cloud_success_rate']*100:.1f}%")
    
    def print_performance_comparison(self) -> None:
        """Print performance comparison between edge and cloud processing."""
        
        logger.info("\n" + "=" * 60)
        logger.info("Performance Comparison")
        logger.info("=" * 60)
        
        # Calculate average latencies
        if self.stats["edge_processed"] > 0:
            avg_edge_latency = self.stats["edge_latency_ms"] / self.stats["edge_processed"]
            logger.info(f"Edge Processing:")
            logger.info(f"  Average latency: {avg_edge_latency:.2f}ms")
            logger.info(f"  Total processed: {self.stats['edge_processed']}")
        
        if self.stats["cloud_processed"] > 0:
            avg_cloud_latency = self.stats["cloud_latency_ms"] / self.stats["cloud_processed"]
            logger.info(f"Cloud Processing:")
            logger.info(f"  Average latency: {avg_cloud_latency:.2f}ms")
            logger.info(f"  Total processed: {self.stats['cloud_processed']}")
        
        # Calculate savings
        if self.stats["edge_processed"] > 0 and self.stats["cloud_processed"] > 0:
            avg_edge_latency = self.stats["edge_latency_ms"] / self.stats["edge_processed"]
            avg_cloud_latency = self.stats["cloud_latency_ms"] / self.stats["cloud_processed"]
            
            latency_reduction = (avg_cloud_latency - avg_edge_latency) / avg_cloud_latency * 100
            bandwidth_savings = self.stats["edge_processed"] / (self.stats["edge_processed"] + self.stats["cloud_processed"]) * 100
            
            logger.info(f"\nSavings:")
            logger.info(f"  Latency reduction: {latency_reduction:.1f}%")
            logger.info(f"  Bandwidth savings: {bandwidth_savings:.1f}%")


async def create_demo_audio(filepath: str = "examples/audio.wav") -> None:
    """Create demo audio file if it doesn't exist."""
    if Path(filepath).exists():
        return
    
    import soundfile as sf
    
    logger.info(f"Creating demo audio file: {filepath}")
    
    # Create 10 seconds of test audio with speech-like patterns
    sample_rate = 44100
    duration = 10
    samples = sample_rate * duration
    
    # Generate simple sine wave "speech"
    t = np.linspace(0, duration, samples)
    audio = np.zeros(samples)
    
    # Add speech segments
    speech_segments = [
        (1.0, 3.0),   # 2 seconds of "speech"
        (4.0, 6.0),   # 2 seconds of "speech"
        (7.0, 9.0),   # 2 seconds of "speech"
    ]
    
    for start, end in speech_segments:
        mask = (t >= start) & (t < end)
        freq = 200 + 100 * np.sin(2 * np.pi * 0.5 * t[mask])
        audio[mask] = 0.3 * np.sin(2 * np.pi * freq * t[mask])
    
    # Add some noise
    audio += 0.01 * np.random.randn(samples)
    
    # Save audio file
    Path(filepath).parent.mkdir(parents=True, exist_ok=True)
    sf.write(filepath, audio.astype(np.float32), sample_rate)
    logger.info(f"Created demo audio: {duration}s with {len(speech_segments)} speech segments")


async def main():
    """Main entry point."""
    
    # Ensure WASM modules directory exists
    wasm_modules_dir = Path("modules/audio")
    if not wasm_modules_dir.exists():
        logger.warning(f"WASM modules directory not found: {wasm_modules_dir}")
        logger.warning("Please build WASM modules first: ./scripts/build-modules.sh")
        
        # Create dummy directory for demo
        wasm_modules_dir.mkdir(parents=True, exist_ok=True)
        
        # Create placeholder WASM files for demo
        for module in ["fast_vad.wasm", "audio_preprocess.wasm", "simple_asr.wasm", 
                      "simple_tts.wasm", "audio_enhance.wasm"]:
            module_path = wasm_modules_dir / module
            if not module_path.exists():
                # Create empty file as placeholder
                module_path.write_bytes(b"")
                logger.info(f"Created placeholder: {module_path}")
    
    # Create demo audio if needed
    await create_demo_audio()
    
    # Check if remote service is running
    remote_host = os.environ.get("REMOTE_HOST", "localhost")
    remote_port = int(os.environ.get("REMOTE_PORT", "50052"))
    
    logger.info(f"Connecting to remote service at {remote_host}:{remote_port}")
    logger.info("If connection fails, start the service with:")
    logger.info("  cd service && python src/server.py")
    
    # Run the hybrid pipeline
    pipeline = HybridSpeechPipeline(remote_host=remote_host, remote_port=remote_port)
    
    try:
        await pipeline.run()
        pipeline.print_performance_comparison()
        
    except Exception as e:
        logger.error(f"Pipeline failed: {e}")
        logger.error("Make sure:")
        logger.error("  1. Remote service is running (cd service && python src/server.py)")
        logger.error("  2. WASM modules are built (./scripts/build-modules.sh)")
        logger.error("  3. Audio input file exists")
        raise


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logger.info("\nPipeline interrupted by user")
    except Exception as e:
        logger.error(f"Fatal error: {e}", exc_info=True)
        sys.exit(1)