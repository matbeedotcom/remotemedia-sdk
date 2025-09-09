"""
WebRTC Server with Pipeline Integration using aiortc.
"""

import asyncio
import logging
import json
import uuid
from typing import Dict, Optional, Any, Callable, List, Set
from dataclasses import dataclass, asdict
from pathlib import Path

from aiortc import RTCPeerConnection, RTCSessionDescription, RTCDataChannel, RTCIceCandidate, RTCConfiguration, RTCIceServer
from aiortc.contrib.media import MediaPlayer, MediaRecorder
from aiohttp import web, WSMsgType
from aiohttp.web import Application, Request, Response, WebSocketResponse
import aiohttp_cors

from ..core.pipeline import Pipeline
from ..core.node import Node
from .pipeline_processor import WebRTCPipelineProcessor

logger = logging.getLogger(__name__)


@dataclass
class WebRTCConfig:
    """Configuration for WebRTC server."""
    host: str = "0.0.0.0"
    port: int = 8080
    stun_servers: List[str] = None
    turn_servers: List[Dict[str, str]] = None
    enable_cors: bool = True
    static_files_path: Optional[str] = None
    
    def __post_init__(self):
        if self.stun_servers is None:
            self.stun_servers = ["stun:stun.l.google.com:19302"]
        if self.turn_servers is None:
            self.turn_servers = []


class WebRTCConnection:
    """Represents a single WebRTC peer connection with pipeline processing."""
    
    def __init__(
        self,
        connection_id: str,
        pc: RTCPeerConnection,
        pipeline_factory: Optional[Callable[[], Pipeline]] = None
    ):
        self.connection_id = connection_id
        self.pc = pc
        self.pipeline_factory = pipeline_factory
        self.pipeline_processor: Optional[WebRTCPipelineProcessor] = None
        self.data_channels: Dict[str, RTCDataChannel] = {}
        self.is_active = True
        
    async def initialize_pipeline(self):
        """Initialize the pipeline processor for this connection."""
        if self.pipeline_factory and not self.pipeline_processor:
            pipeline = self.pipeline_factory()
            self.pipeline_processor = WebRTCPipelineProcessor(
                pipeline=pipeline,
                connection_id=self.connection_id
            )
            await self.pipeline_processor.initialize()
            logger.info(f"Pipeline processor initialized for connection {self.connection_id}")
    
    async def cleanup(self):
        """Clean up the connection and its resources."""
        self.is_active = False
        
        if self.pipeline_processor:
            await self.pipeline_processor.cleanup()
            self.pipeline_processor = None
            
        for channel in self.data_channels.values():
            if channel.readyState == "open":
                channel.close()
        
        await self.pc.close()
        logger.info(f"Connection {self.connection_id} cleaned up")


class WebRTCServer:
    """
    WebRTC Server with integrated pipeline processing capabilities.
    
    This server allows clients to establish WebRTC connections and process
    audio, video, and data streams through RemoteMedia pipelines.
    """
    
    def __init__(
        self,
        config: Optional[WebRTCConfig] = None,
        pipeline_factory: Optional[Callable[[], Pipeline]] = None
    ):
        """
        Initialize the WebRTC server.
        
        Args:
            config: Server configuration
            pipeline_factory: Function that creates pipeline instances for each connection
        """
        self.config = config or WebRTCConfig()
        self.pipeline_factory = pipeline_factory
        self.connections: Dict[str, WebRTCConnection] = {}
        self.websockets: Dict[str, WebSocketResponse] = {}
        self.app: Optional[Application] = None
        self.runner: Optional[web.AppRunner] = None
        
    def set_pipeline_factory(self, factory: Callable[[], Pipeline]):
        """Set the pipeline factory for creating processing pipelines."""
        self.pipeline_factory = factory
        
    async def start(self):
        """Start the WebRTC server."""
        self.app = self._create_app()
        
        self.runner = web.AppRunner(self.app)
        await self.runner.setup()
        
        site = web.TCPSite(
            self.runner,
            host=self.config.host,
            port=self.config.port
        )
        await site.start()
        
        logger.info(f"WebRTC server started on {self.config.host}:{self.config.port}")
        
    async def stop(self):
        """Stop the WebRTC server and clean up all connections."""
        # Clean up all connections
        for connection in list(self.connections.values()):
            await connection.cleanup()
        self.connections.clear()
        
        # Close all websockets
        for ws in list(self.websockets.values()):
            if not ws.closed:
                await ws.close()
        self.websockets.clear()
        
        # Stop the web server
        if self.runner:
            await self.runner.cleanup()
            
        logger.info("WebRTC server stopped")
        
    def _create_app(self) -> Application:
        """Create the aiohttp application with routes."""
        app = Application()
        
        # Add routes
        app.router.add_get("/ws", self._websocket_handler)
        app.router.add_get("/health", self._health_handler)
        app.router.add_get("/connections", self._connections_handler)
        
        # Serve static files if configured
        if self.config.static_files_path:
            static_path = Path(self.config.static_files_path)
            if static_path.exists():
                app.router.add_static("/", static_path, name="static")
                app.router.add_get("/", self._index_handler)
        
        # Configure CORS if enabled
        if self.config.enable_cors:
            cors = aiohttp_cors.setup(app, defaults={
                "*": aiohttp_cors.ResourceOptions(
                    allow_credentials=True,
                    expose_headers="*",
                    allow_headers="*",
                    allow_methods="*"
                )
            })
            for route in list(app.router.routes()):
                cors.add(route)
        
        return app
    
    async def _websocket_handler(self, request: Request) -> WebSocketResponse:
        """Handle WebSocket connections for signaling."""
        ws = WebSocketResponse()
        await ws.prepare(request)
        
        connection_id = str(uuid.uuid4())
        self.websockets[connection_id] = ws
        
        logger.info(f"WebSocket connection established: {connection_id}")
        
        try:
            async for msg in ws:
                if msg.type == WSMsgType.TEXT:
                    try:
                        data = json.loads(msg.data)
                        await self._handle_signaling_message(connection_id, data)
                    except json.JSONDecodeError:
                        logger.error(f"Invalid JSON from {connection_id}: {msg.data}")
                elif msg.type == WSMsgType.ERROR:
                    logger.error(f"WebSocket error from {connection_id}: {ws.exception()}")
                    break
        except Exception as e:
            logger.error(f"WebSocket handler error for {connection_id}: {e}")
        finally:
            # Clean up on disconnect
            if connection_id in self.websockets:
                del self.websockets[connection_id]
            if connection_id in self.connections:
                await self.connections[connection_id].cleanup()
                del self.connections[connection_id]
            logger.info(f"WebSocket connection closed: {connection_id}")
        
        return ws
    
    async def _handle_signaling_message(self, connection_id: str, data: Dict[str, Any]):
        """Handle signaling messages from clients."""
        message_type = data.get("type")
        
        if message_type == "offer":
            await self._handle_offer(connection_id, data)
        elif message_type == "answer":
            await self._handle_answer(connection_id, data)
        elif message_type == "ice-candidate":
            await self._handle_ice_candidate(connection_id, data)
        elif message_type == "close":
            await self._handle_close(connection_id)
        else:
            logger.warning(f"Unknown message type from {connection_id}: {message_type}")
    
    async def _handle_offer(self, connection_id: str, data: Dict[str, Any]):
        """Handle WebRTC offer from client."""
        try:
            # Create peer connection with proper configuration
            ice_servers = []
            
            # Add STUN servers
            for stun_url in self.config.stun_servers:
                ice_servers.append(RTCIceServer(urls=[stun_url]))
            
            # Add TURN servers
            for turn_config in self.config.turn_servers:
                if isinstance(turn_config, dict):
                    ice_servers.append(RTCIceServer(
                        urls=[turn_config["urls"]],
                        username=turn_config.get("username"),
                        credential=turn_config.get("credential")
                    ))
            
            configuration = RTCConfiguration(iceServers=ice_servers)
            pc = RTCPeerConnection(configuration=configuration)
            
            # Create connection wrapper
            connection = WebRTCConnection(connection_id, pc, self.pipeline_factory)
            self.connections[connection_id] = connection
            
            # Initialize pipeline BEFORE setting up event handlers
            await connection.initialize_pipeline()
            
            # Set up event handlers
            self._setup_peer_connection_handlers(connection)
            
            # Process the offer - extract only the SDP and type
            offer = RTCSessionDescription(
                sdp=data["sdp"],
                type=data["type"]
            )
            await pc.setRemoteDescription(offer)
            
            # Add audio output track for sending generated audio back to client
            if connection.pipeline_processor and connection.pipeline_processor.audio_output_track:
                pc.addTrack(connection.pipeline_processor.audio_output_track)
                logger.info(f"Added audio output track to connection {connection_id}")
            
            # Create answer
            answer = await pc.createAnswer()
            await pc.setLocalDescription(answer)
            
            # Send answer back
            await self._send_signaling_message(connection_id, {
                "type": "answer",
                "sdp": pc.localDescription.sdp
            })
            
            logger.info(f"Created answer for connection {connection_id}")
            
        except Exception as e:
            logger.error(f"Error handling offer from {connection_id}: {e}")
            await self._send_signaling_message(connection_id, {
                "type": "error",
                "message": str(e)
            })
    
    async def _handle_answer(self, connection_id: str, data: Dict[str, Any]):
        """Handle WebRTC answer from client."""
        if connection_id not in self.connections:
            logger.warning(f"Received answer for unknown connection: {connection_id}")
            return
        
        try:
            connection = self.connections[connection_id]
            answer = RTCSessionDescription(
                sdp=data["sdp"],
                type=data["type"]
            )
            await connection.pc.setRemoteDescription(answer)
            logger.info(f"Set remote description for connection {connection_id}")
            
        except Exception as e:
            logger.error(f"Error handling answer from {connection_id}: {e}")
    
    async def _handle_ice_candidate(self, connection_id: str, data: Dict[str, Any]):
        """Handle ICE candidate from client."""
        if connection_id not in self.connections:
            logger.warning(f"Received ICE candidate for unknown connection: {connection_id}")
            return
        
        try:
            connection = self.connections[connection_id]
            candidate_data = data.get("candidate")
            if candidate_data:
                candidate_line = candidate_data.get("candidate")
                sdp_mid = candidate_data.get("sdpMid") 
                sdp_mline_index = candidate_data.get("sdpMLineIndex")
                
                if candidate_line:
                    # Parse the candidate string manually for aiortc
                    # Format: "candidate:foundation component protocol priority ip port typ type ..."
                    parts = candidate_line.split()
                    if len(parts) >= 8 and parts[0].startswith("candidate:"):
                        try:
                            foundation = parts[0][10:]  # Remove "candidate:" prefix
                            component = int(parts[1])
                            protocol = parts[2]
                            priority = int(parts[3])
                            ip = parts[4]
                            port = int(parts[5])
                            typ = parts[7]  # parts[6] is "typ"
                            
                            # Create RTCIceCandidate with parsed values
                            candidate = RTCIceCandidate(
                                component=component,
                                foundation=foundation,
                                ip=ip,
                                port=port,
                                priority=priority,
                                protocol=protocol,
                                type=typ,
                                sdpMid=sdp_mid,
                                sdpMLineIndex=sdp_mline_index
                            )
                            
                            await connection.pc.addIceCandidate(candidate)
                            logger.debug(f"Added ICE candidate for connection {connection_id}")
                        except (ValueError, IndexError) as e:
                            logger.warning(f"Failed to parse ICE candidate '{candidate_line}': {e}")
                    else:
                        logger.warning(f"Invalid ICE candidate format: {candidate_line}")
            
        except Exception as e:
            logger.error(f"Error handling ICE candidate from {connection_id}: {e}")
    
    async def _handle_close(self, connection_id: str):
        """Handle connection close request."""
        if connection_id in self.connections:
            await self.connections[connection_id].cleanup()
            del self.connections[connection_id]
            logger.info(f"Closed connection {connection_id}")
    
    def _setup_peer_connection_handlers(self, connection: WebRTCConnection):
        """Set up event handlers for a peer connection."""
        
        @connection.pc.on("datachannel")
        def on_datachannel(channel: RTCDataChannel):
            logger.info(f"Data channel '{channel.label}' created for {connection.connection_id}")
            connection.data_channels[channel.label] = channel
            
            @channel.on("open")
            def on_open():
                logger.info(f"Data channel '{channel.label}' opened for {connection.connection_id}")
            
            @channel.on("message")
            def on_message(message):
                logger.debug(f"Data channel message on '{channel.label}': {message}")
                # Forward message to pipeline processor
                if connection.pipeline_processor:
                    asyncio.create_task(
                        connection.pipeline_processor.process_data_message(channel.label, message)
                    )
        
        @connection.pc.on("track")
        def on_track(track):
            logger.info(f"Track received: {track.kind} for {connection.connection_id}")
            
            # Forward track to pipeline processor
            if connection.pipeline_processor:
                asyncio.create_task(
                    connection.pipeline_processor.add_track(track)
                )
                logger.info(f"Forwarded {track.kind} track to pipeline processor for {connection.connection_id}")
            else:
                logger.warning(f"No pipeline processor available for track {track.kind} on {connection.connection_id}")
            
            @track.on("ended")
            def on_ended():
                logger.info(f"Track ended: {track.kind} for {connection.connection_id}")
                if connection.pipeline_processor:
                    asyncio.create_task(
                        connection.pipeline_processor.remove_track(track)
                    )
        
        @connection.pc.on("icecandidate")
        async def on_icecandidate(candidate):
            if candidate:
                logger.debug(f"ICE candidate generated for {connection.connection_id}")
                await self._send_signaling_message(connection.connection_id, {
                    "type": "ice-candidate",
                    "candidate": {
                        "candidate": f"candidate:{candidate.foundation} {candidate.component} {candidate.protocol} {candidate.priority} {candidate.ip} {candidate.port} typ {candidate.type}",
                        "sdpMid": candidate.sdpMid,
                        "sdpMLineIndex": candidate.sdpMLineIndex
                    }
                })
        
        @connection.pc.on("connectionstatechange")
        async def on_connectionstatechange():
            state = connection.pc.connectionState
            logger.info(f"Connection state changed to {state} for {connection.connection_id}")
            
            if state == "failed" or state == "closed":
                if connection.connection_id in self.connections:
                    await connection.cleanup()
                    del self.connections[connection.connection_id]
    
    async def _send_signaling_message(self, connection_id: str, message: Dict[str, Any]):
        """Send a signaling message to a client."""
        if connection_id in self.websockets:
            ws = self.websockets[connection_id]
            if not ws.closed:
                try:
                    await ws.send_str(json.dumps(message))
                except Exception as e:
                    logger.error(f"Error sending message to {connection_id}: {e}")
    
    async def _health_handler(self, request: Request) -> Response:
        """Health check endpoint."""
        return web.json_response({
            "status": "healthy",
            "active_connections": len(self.connections),
            "websocket_connections": len(self.websockets)
        })
    
    async def _connections_handler(self, request: Request) -> Response:
        """Get information about active connections."""
        connections_info = []
        for conn_id, conn in self.connections.items():
            connections_info.append({
                "id": conn_id,
                "state": conn.pc.connectionState,
                "ice_state": conn.pc.iceConnectionState,
                "data_channels": list(conn.data_channels.keys()),
                "has_pipeline": conn.pipeline_processor is not None,
                "is_active": conn.is_active
            })
        
        return web.json_response({
            "connections": connections_info,
            "total": len(connections_info)
        })
    
    async def _index_handler(self, request: Request) -> Response:
        """Serve index.html if static files are configured."""
        if self.config.static_files_path:
            index_path = Path(self.config.static_files_path) / "index.html"
            if index_path.exists():
                return web.FileResponse(index_path)
        
        return web.Response(
            text="WebRTC Server is running. Configure static_files_path to serve client files.",
            content_type="text/plain"
        )
    
    def broadcast_to_connections(self, message: Dict[str, Any], connection_filter: Optional[Callable[[WebRTCConnection], bool]] = None):
        """
        Broadcast a message to all or filtered connections.
        
        Args:
            message: Message to broadcast
            connection_filter: Optional function to filter which connections receive the message
        """
        for conn_id, connection in self.connections.items():
            if connection_filter is None or connection_filter(connection):
                # Send via data channel if available
                for channel in connection.data_channels.values():
                    if channel.readyState == "open":
                        try:
                            channel.send(json.dumps(message))
                        except Exception as e:
                            logger.error(f"Error broadcasting to {conn_id}: {e}")
    
    async def get_connection_stats(self, connection_id: str) -> Optional[Dict[str, Any]]:
        """Get statistics for a specific connection."""
        if connection_id not in self.connections:
            return None
        
        connection = self.connections[connection_id]
        try:
            stats = await connection.pc.getStats()
            return {
                "connection_id": connection_id,
                "connection_state": connection.pc.connectionState,
                "ice_state": connection.pc.iceConnectionState,
                "stats": stats
            }
        except Exception as e:
            logger.error(f"Error getting stats for {connection_id}: {e}")
            return None