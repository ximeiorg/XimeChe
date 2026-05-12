#!/bin/bash
# Test VirtualKeyboard flow manually
# Creates socketpair, sets WAYLAND_SOCKET, launches xime

set -e

echo "Testing xime VirtualKeyboard flow..."

# Create socketpair
echo "Creating socketpair..."
python3 -c "
import socket
import os

# Create socket pair
sock1, sock2 = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)

# Get file descriptors
fd1 = sock1.fileno()
fd2 = sock2.fileno()

print(f'Socket FD for launcher: {fd1}')
print(f'Socket FD for server: {fd2}')

# Set environment variable
os.environ['WAYLAND_SOCKET'] = str(fd1)

# Export to parent shell
import sys
sys.stdout.flush()

# Keep socket alive
sock1.close()
" &

sleep 1

# This won't work because we can't pass fd to another process this way
# Let's try another approach - use the actual KWin socket

echo ""
echo "Alternative: Check if daemon receives connection from existing Wayland socket"
echo "Starting daemon..."

# Kill existing
pkill xime-daemon 2>/dev/null || true
sleep 1

# Start daemon
~/.local/bin/xime-daemon &>/tmp/daemon-test.log &
sleep 2

echo "Daemon started. DBus service:"
qdbus org.xime.Xime /org/xime/Xime org.xime.Xime.Controller 2>&1 | head -5

echo ""
echo "Daemon log:"
cat /tmp/daemon-test.log | grep DEBUG | tail -5

echo ""
echo "Now waiting for KWin to trigger..."
echo "Open a text-input capable app and check logs"