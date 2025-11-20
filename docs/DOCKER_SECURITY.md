# Docker Container Security Hardening (T057)

This document describes the security hardening features implemented for Docker-based Python nodes in the RemoteMedia SDK.

## Overview

All Docker containers running Python nodes are secured by default with:
- Dropped Linux capabilities (principle of least privilege)
- Read-only root filesystem with tmpfs for writable areas
- Non-root user execution
- Privilege escalation prevention
- AppArmor/SELinux profiles when available

## Security Configuration

### Default Security Settings

By default, containers are created with the following security settings:

```json
{
  "security": {
    "cap_drop": ["ALL"],
    "cap_add": ["IPC_LOCK", "SYS_NICE"],
    "read_only_rootfs": true,
    "security_opt": ["no-new-privileges:true"],
    "user": "1000",
    "group": "1000",
    "enable_apparmor": true,
    "apparmor_profile": "docker-default",
    "tmpfs_mounts": ["/tmp", "/var/tmp", "/run"]
  }
}
```

### Linux Capabilities

#### Dropped Capabilities
All Linux capabilities are dropped by default (`cap_drop: ["ALL"]`), following the principle of least privilege.

#### Added Capabilities
Only essential capabilities for IPC operations are added back:

- **IPC_LOCK**: Required for iceoryx2 shared memory operations. Allows locking memory pages to prevent swapping.
- **SYS_NICE**: Required for process priority management in real-time pipelines. Allows adjusting process scheduling priorities.

**Security Note**: These capabilities are minimal and necessary for the system to function. The risk is mitigated by:
- Memory limits preventing abuse of IPC_LOCK
- Container isolation limiting scope of SYS_NICE

### Read-Only Root Filesystem

Containers run with read-only root filesystems (`read_only_rootfs: true`) to prevent:
- Modification of system binaries
- Installation of malicious software
- Tampering with application code

#### Writable Areas via tmpfs

Writable directories are provided via tmpfs mounts with restrictive options:

```
/tmp, /var/tmp, /run: rw,noexec,nosuid,size=64m
```

- **noexec**: Prevents execution of binaries from tmpfs
- **nosuid**: Ignores set-user-ID and set-group-ID bits
- **size=64m**: Limits tmpfs size to prevent denial-of-service

### Non-Root User Execution

All containers run as non-root user (default: `1000:1000`) to:
- Limit impact of container breakout vulnerabilities
- Prevent privilege escalation within the container
- Follow security best practices

**Important**: Docker images must be compatible with non-root execution. Ensure file permissions are set correctly during image build.

### Privilege Escalation Prevention

The `no-new-privileges` security option prevents processes from gaining additional privileges via:
- setuid/setgid executables
- File capabilities
- Ambient capabilities

This ensures processes cannot escalate privileges even if vulnerabilities exist.

### AppArmor/SELinux Profiles

When available, containers use the `docker-default` AppArmor profile, which provides:
- Mandatory access control restrictions
- Protection against common container escape techniques
- Additional filesystem and network restrictions

## Security Presets

### Default Security (Recommended)

Balanced security with necessary capabilities for IPC:

```rust
use remotemedia_runtime_core::python::multiprocess::docker_support::SecurityConfig;

let security = SecurityConfig::default();
```

### Strict Security (Maximum Hardening)

Minimal privileges - only IPC_LOCK capability:

```rust
let security = SecurityConfig::strict();
```

This preset:
- Drops SYS_NICE capability (may impact real-time scheduling)
- Uses `nobody:nogroup` (UID/GID 65534)
- Enforces all security restrictions

### Permissive Security (Development Only)

**WARNING**: Only use in trusted development environments!

```rust
let security = SecurityConfig::permissive();
```

This preset disables all security hardening and should **NEVER** be used in production.

## Configuration Examples

### Pipeline Manifest with Security

```json
{
  "nodes": [
    {
      "id": "secure_node",
      "node_type": "CustomNode",
      "docker": {
        "python_version": "3.11",
        "memory_mb": 2048,
        "cpu_cores": 2.0,
        "security": {
          "cap_drop": ["ALL"],
          "cap_add": ["IPC_LOCK", "SYS_NICE"],
          "read_only_rootfs": true,
          "user": "1000",
          "group": "1000"
        }
      }
    }
  ]
}
```

### Rust Configuration

```rust
use remotemedia_runtime_core::python::multiprocess::docker_support::{
    DockerNodeConfig, SecurityConfig
};

let config = DockerNodeConfig {
    python_version: "3.11".to_string(),
    memory_mb: 2048,
    cpu_cores: 2.0,
    security: SecurityConfig {
        cap_drop: vec!["ALL".to_string()],
        cap_add: vec!["IPC_LOCK".to_string(), "SYS_NICE".to_string()],
        read_only_rootfs: true,
        security_opt: vec!["no-new-privileges:true".to_string()],
        user: "1000".to_string(),
        group: "1000".to_string(),
        enable_apparmor: true,
        apparmor_profile: "docker-default".to_string(),
        tmpfs_mounts: vec![
            "/tmp".to_string(),
            "/var/tmp".to_string(),
            "/run".to_string(),
        ],
    },
    ..Default::default()
};
```

## Security Implications and Considerations

### IPC_LOCK Capability

**Risk**: Allows locking memory pages, which could be used to consume system memory.

**Mitigation**:
- Memory limits enforced at container level
- iceoryx2 manages shared memory responsibly
- Process isolation via containers

### SYS_NICE Capability

**Risk**: Allows changing process priorities, potential for local denial-of-service.

**Mitigation**:
- Container isolation limits scope
- CPU limits enforced at container level
- Real-time priority adjustments are necessary for audio processing

### Volume Mounts (/tmp and /dev)

**Risk**: Shared host directories could expose sensitive data or allow container escape.

**Mitigation**:
- Only necessary directories mounted for IPC
- Read-only mounts where possible
- Host-level permissions control access
- Consider using namespaced IPC in high-security environments

### Read-Only Root Filesystem

**Impact**: Applications that write to unexpected locations will fail.

**Solution**: Add additional tmpfs mounts as needed:

```json
{
  "security": {
    "tmpfs_mounts": ["/tmp", "/var/tmp", "/run", "/app/cache"]
  }
}
```

### Non-Root User

**Impact**: Applications must be compatible with non-root execution.

**Solution**: In your Dockerfile:
```dockerfile
# Create user with specific UID/GID
RUN useradd -u 1000 -U appuser

# Set proper permissions
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser
```

## Validation and Monitoring

### Security Configuration Validation

The system automatically validates security configurations:

```rust
config.validate()?;  // Returns error for invalid configurations
```

Common validation warnings:
- Running as root user
- No capabilities dropped
- Read-only rootfs disabled without tmpfs mounts

### Security Logging

All security settings are logged at container creation:

```
INFO: Applying container security hardening
  cap_drop: ["ALL"]
  cap_add: ["IPC_LOCK", "SYS_NICE"]
  read_only_rootfs: true
  user: 1000:1000
```

## Best Practices

1. **Always use default security settings** unless you have a specific reason to modify them

2. **Never disable security features in production** - use `SecurityConfig::permissive()` only in development

3. **Test with strict security first** - Use `SecurityConfig::strict()` to identify minimum required capabilities

4. **Review logs** - Monitor security warnings for misconfigurations

5. **Keep images updated** - Regularly update base images and dependencies

6. **Minimize privileges** - Only add capabilities that are absolutely necessary

7. **Use specific user IDs** - Don't rely on default user IDs, specify explicitly

8. **Document security requirements** - If you need additional capabilities, document why

## Troubleshooting

### Container fails with permission errors

**Cause**: Application trying to write to read-only filesystem

**Solution**: Add writable area via tmpfs:
```json
"tmpfs_mounts": ["/tmp", "/app/cache"]
```

### IPC operations fail

**Cause**: Missing IPC_LOCK capability

**Solution**: Ensure IPC_LOCK is in cap_add:
```json
"cap_add": ["IPC_LOCK"]
```

### Process priority cannot be adjusted

**Cause**: Missing SYS_NICE capability

**Solution**: Add SYS_NICE to cap_add:
```json
"cap_add": ["IPC_LOCK", "SYS_NICE"]
```

### AppArmor profile not found

**Cause**: AppArmor not installed or profile missing

**Solution**: Either install AppArmor or disable:
```json
"enable_apparmor": false
```

## References

- [Docker Security Best Practices](https://docs.docker.com/engine/security/)
- [Linux Capabilities](https://man7.org/linux/man-pages/man7/capabilities.7.html)
- [AppArmor Documentation](https://gitlab.com/apparmor/apparmor/-/wikis/home)
- [iceoryx2 Security Considerations](https://github.com/eclipse-iceoryx/iceoryx2)

## See Also

- [RemoteMedia SDK Architecture](../CLAUDE.md)
- [Multiprocess Executor](../specs/002-grpc-multiprocess-integration/)
- [Docker Support Module](../runtime-core/src/python/multiprocess/docker_support.rs)
