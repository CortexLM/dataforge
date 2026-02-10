#!/bin/bash
# setup_task_env.sh - Setup Docker environment for a dataforge task
#
# Usage: ./setup_task_env.sh <task_directory>
# Example: ./setup_task_env.sh fc132552-291c-428d-8430-14ed0db1e1b8

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <task_directory>"
    echo ""
    echo "Available tasks:"
    for dir in */; do
        if [ -f "${dir}task.yaml" ]; then
            echo "  - ${dir%/}"
        fi
    done
    exit 1
fi

TASK_DIR="$1"
TASK_ID=$(basename "$TASK_DIR")

if [ ! -f "$TASK_DIR/task.yaml" ]; then
    echo "Error: task.yaml not found in $TASK_DIR"
    exit 1
fi

echo "=========================================="
echo "Setting up environment for task: $TASK_ID"
echo "=========================================="

# Extract category from task.yaml
CATEGORY=$(grep "category:" "$TASK_DIR/task.yaml" | head -1 | awk '{print $2}')
echo "Category: $CATEGORY"

# Create Dockerfile based on category
cat > "$TASK_DIR/Dockerfile" << 'DOCKERFILE'
FROM ubuntu:24.04

# Prevent interactive prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install base tools
RUN apt-get update && apt-get install -y \
    python3 python3-pip python3-yaml \
    curl wget vim nano jq \
    git htop tree \
    net-tools iputils-ping dnsutils \
    && rm -rf /var/lib/apt/lists/*

# Create user
RUN useradd -m -s /bin/bash agent && \
    echo "agent ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Create workspace structure
RUN mkdir -p /workspace /output /root/monitoring

# Set environment
ENV DATAFORGE_WORKSPACE=/workspace
ENV TERM=xterm-256color

WORKDIR /workspace

# Copy task files
COPY prompt.md task.yaml ./
COPY solution.sh ./reference_solution.sh

# Default command
CMD ["/bin/bash"]
DOCKERFILE

# Create docker-compose.yaml
cat > "$TASK_DIR/docker-compose.yaml" << COMPOSE
version: '3.8'

services:
  task-env:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: dataforge-${TASK_ID:0:12}
    hostname: task-runner
    volumes:
      - ./workspace:/workspace
      - ./output:/output
    environment:
      - TASK_ID=${TASK_ID}
      - DATAFORGE_WORKSPACE=/workspace
    stdin_open: true
    tty: true
    # Resource limits (adjust based on task difficulty)
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 1G
COMPOSE

# Create directories
mkdir -p "$TASK_DIR/workspace"
mkdir -p "$TASK_DIR/output"

# Copy verification script
cp verify_solution.py "$TASK_DIR/" 2>/dev/null || true

# Create a run script
cat > "$TASK_DIR/run.sh" << 'RUNSCRIPT'
#!/bin/bash
# Run the task environment

echo "Starting task environment..."
docker-compose up -d

echo ""
echo "Environment ready! Commands:"
echo "  - Enter container:  docker exec -it dataforge-${TASK_ID:0:12} bash"
echo "  - View prompt:      docker exec dataforge-${TASK_ID:0:12} cat /workspace/prompt.md"
echo "  - Stop:             docker-compose down"
echo ""
echo "To verify solution after agent completes:"
echo "  docker exec dataforge-${TASK_ID:0:12} python3 /workspace/verify_solution.py /workspace /output"
RUNSCRIPT
chmod +x "$TASK_DIR/run.sh"

# Create verify script wrapper
cat > "$TASK_DIR/verify.sh" << 'VERIFYSCRIPT'
#!/bin/bash
# Verify the solution in the container

CONTAINER="dataforge-${TASK_ID:0:12}"

if ! docker ps --format '{{.Names}}' | grep -q "$CONTAINER"; then
    echo "Error: Container $CONTAINER is not running"
    echo "Start it first with: docker-compose up -d"
    exit 1
fi

echo "Running verification..."
docker exec "$CONTAINER" python3 /workspace/verify_solution.py /workspace /output
VERIFYSCRIPT
chmod +x "$TASK_DIR/verify.sh"

echo ""
echo "=========================================="
echo "Setup complete!"
echo "=========================================="
echo ""
echo "Files created:"
echo "  - $TASK_DIR/Dockerfile"
echo "  - $TASK_DIR/docker-compose.yaml"
echo "  - $TASK_DIR/run.sh"
echo "  - $TASK_DIR/verify.sh"
echo ""
echo "Next steps:"
echo "  1. cd $TASK_DIR"
echo "  2. ./run.sh                    # Start the environment"
echo "  3. docker exec -it dataforge-${TASK_ID:0:12} bash"
echo "  4. # Agent works on the task..."
echo "  5. ./verify.sh                 # Verify the solution"
echo ""
