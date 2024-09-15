#!/bin/bash

set -e

echo "Starting RLSR uninstallation script..."

# Check if rlsr is installed
if ! command -v rlsr &>/dev/null; then
    echo "Error: rlsr is not found in the system PATH."
    echo "It may not be installed or might be installed in a non-standard location."
    exit 1
fi

# Get the path of the rlsr binary
RLSR_PATH=$(which rlsr)
echo "RLSR found at: ${RLSR_PATH}"

# Remove the binary
echo "Removing RLSR binary (requires sudo)..."
if sudo rm "${RLSR_PATH}"; then
    echo "RLSR binary successfully removed."
else
    echo "Error: Failed to remove RLSR binary. You may need to remove it manually."
    exit 1
fi

# Verify uninstallation
if command -v rlsr &>/dev/null; then
    echo "Warning: rlsr command is still accessible in the PATH."
    echo "It might be installed in multiple locations. Please check and remove manually if needed."
else
    echo "RLSR has been successfully uninstalled from the system."
fi

echo "Uninstallation process completed."
