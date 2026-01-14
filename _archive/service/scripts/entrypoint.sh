#!/bin/bash

# Fix ownership of mounted cache directories
echo "Fixing cache directory permissions..."
chown -R remotemedia:remotemedia /home/remotemedia/.cache/huggingface /home/remotemedia/.cache/torch /app/cache/models 2>/dev/null || true

# Ensure the remotemedia user owns their home directory
chown -R remotemedia:remotemedia /home/remotemedia 2>/dev/null || true

# Switch to remotemedia user and execute the command
exec gosu remotemedia "$@"