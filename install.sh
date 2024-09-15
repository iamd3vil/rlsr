#!/bin/bash

set -e

echo "Starting RLSR installation script..."

# Function to get the latest release version
get_latest_version() {
    curl -s https://api.github.com/repos/iamd3vil/rlsr/releases/latest | sed -n 's/.*"tag_name": "\(.*\)".*/\1/p'
}

# Detect OS and architecture
OS=$(uname)
ARCH=$(uname -m)

echo "Detected OS: $OS"
echo "Detected architecture: $ARCH"

# Check if the system is supported
if [[ "$OS" == "Linux" && "$ARCH" == "x86_64" ]]; then
    echo "Linux x86_64 system detected."
    DOWNLOAD_SUFFIX="linux-x86_64.zip"
elif [[ "$OS" == "Darwin" && "$ARCH" == "arm64" ]]; then
    echo "macOS ARM64 system detected."
    DOWNLOAD_SUFFIX="macos-arm64.zip"
else
    echo "Error: Unsupported system. This script supports Linux x86_64 and macOS ARM64 only."
    exit 1
fi

# Get the latest version
echo "Fetching latest release version from GitHub..."
VERSION=$(get_latest_version)
echo "Latest version: ${VERSION}"

# Construct the download URL
DOWNLOAD_URL="https://github.com/iamd3vil/rlsr/releases/download/${VERSION}/rlsr-${VERSION}-${DOWNLOAD_SUFFIX}"
echo "Download URL: ${DOWNLOAD_URL}"

# Create a temporary directory
echo "Creating temporary directory..."
TMP_DIR=$(mktemp -d)
echo "Temporary directory created: ${TMP_DIR}"

# Download the zip file
echo "Downloading rlsr ${VERSION}..."
curl -L -o "${TMP_DIR}/rlsr.zip" "${DOWNLOAD_URL}"
echo "Download completed."

# Unzip the file
echo "Extracting rlsr binary..."
unzip -q "${TMP_DIR}/rlsr.zip" -d "${TMP_DIR}"
echo "Extraction completed."

# Make the binary executable
echo "Setting executable permissions..."
chmod +x "${TMP_DIR}/rlsr"
echo "Permissions set."

# Move the binary to /usr/local/bin (requires sudo)
echo "Installing rlsr to /usr/local/bin (requires sudo)..."
sudo mv "${TMP_DIR}/rlsr" /usr/local/bin/
echo "Installation completed."

# Verify installation
echo "Verifying installation..."
if command -v rlsr &>/dev/null; then
    echo "RLSR successfully installed and accessible from PATH."
    rlsr --version
else
    echo "Error: RLSR installation could not be verified. Please check your PATH."
fi

# Clean up
echo "Cleaning up temporary files..."
rm -rf "${TMP_DIR}"
echo "Cleanup completed."

echo "RLSR ${VERSION} has been successfully installed!"
