import asyncio
from typing import AsyncGenerator, Any
import logging
from . import remote_pb2
from .serialization import CloudPickleSerializer
import inspect
from asyncstdlib import anext

class ObjectExecutionServicer:
    def __init__(self, obj, serializer):
        self.obj = obj
        self.serializer = serializer

    async def StreamObject(self, request_iterator, context):
        try:
            # The first request contains the method name
            first_request = await anext(request_iterator)
            method_name = first_request.method_name
            method_to_call = getattr(self.obj, method_name)
            
            # For streaming methods, we expect them to be async generators
            if inspect.isasyncgenfunction(method_to_call):
                async def input_generator():
                    # The first request's args might be part of the stream
                    if first_request.args:
                        args, _ = self.serializer.deserialize(first_request.args)
                        yield args[0]
                    # Process subsequent requests in the stream
                    async for request in request_iterator:
                        args, _ = self.serializer.deserialize(request.args)
                        yield args[0]

                response_generator = method_to_call(input_generator())
                async for response_data in response_generator:
                    serialized_response, _ = self.serializer.serialize(response_data)
                    yield remote_pb2.StreamObjectResponse(result=serialized_response)
            
            else: # For regular, non-streaming async methods
                args, kwargs = self.serializer.deserialize(first_request.args)
                result = await method_to_call(*args, **kwargs)
                serialized_result, _ = self.serializer.serialize(result)
                yield remote_pb2.StreamObjectResponse(result=serialized_result)

        except StopAsyncIteration:
            pass # Client closed the stream or it was a unary call that finished
        except Exception as e:
            logging.error(f"Error during StreamObject execution with object type {type(self.obj).__name__}: {e}", exc_info=True)
            # You might want to send an error back to the client here
            # For now, just logging and closing.
            pass 