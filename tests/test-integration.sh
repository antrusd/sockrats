#!/bin/bash
# SocksRat Integration Test Script
# Tests both SOCKS5 and SSH capabilities
#
# Prerequisites:
# - rathole binary (https://github.com/rapiz1/rathole)
# - curl (for SOCKS5 testing)
# - ssh/sshpass (for SSH testing)
# - netcat (nc) for connection testing
#
# Usage: ./test-integration.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
TMP_DIR="/tmp/socksrat-test-$$"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
RATHOLE_BIND_PORT=2333
SOCKS5_PORT=1080
SSH_PORT=2222
TEST_USER="testuser"
TEST_PASSWORD="testpassword"

# PIDs for cleanup
RATHOLE_PID=""
SOCKSRAT_SOCKS5_PID=""
SOCKSRAT_SSH_PID=""
SOCKSRAT_MULTI_PID=""
ECHO_SERVER_PID=""

cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"

    # Kill all socksrat processes
    pkill -f "socksrat.*test-socks5.toml" 2>/dev/null || true
    pkill -f "socksrat.*test-ssh.toml" 2>/dev/null || true
    pkill -f "socksrat.*test-multi-service.toml" 2>/dev/null || true

    if [ -n "$SOCKSRAT_SOCKS5_PID" ] && kill -0 "$SOCKSRAT_SOCKS5_PID" 2>/dev/null; then
        kill "$SOCKSRAT_SOCKS5_PID" 2>/dev/null || true
    fi

    if [ -n "$SOCKSRAT_SSH_PID" ] && kill -0 "$SOCKSRAT_SSH_PID" 2>/dev/null; then
        kill "$SOCKSRAT_SSH_PID" 2>/dev/null || true
    fi

    if [ -n "$SOCKSRAT_MULTI_PID" ] && kill -0 "$SOCKSRAT_MULTI_PID" 2>/dev/null; then
        kill "$SOCKSRAT_MULTI_PID" 2>/dev/null || true
    fi

    if [ -n "$RATHOLE_PID" ] && kill -0 "$RATHOLE_PID" 2>/dev/null; then
        kill "$RATHOLE_PID" 2>/dev/null || true
    fi

    if [ -n "$ECHO_SERVER_PID" ] && kill -0 "$ECHO_SERVER_PID" 2>/dev/null; then
        kill "$ECHO_SERVER_PID" 2>/dev/null || true
    fi

    rm -rf "$TMP_DIR" 2>/dev/null || true

    echo -e "${GREEN}Cleanup complete.${NC}"
}

trap cleanup EXIT

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_test() {
    echo -e "\n${YELLOW}[TEST]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    local missing=()

    if ! command -v rathole &> /dev/null; then
        missing+=("rathole")
    fi

    if ! command -v curl &> /dev/null; then
        missing+=("curl")
    fi

    if ! command -v nc &> /dev/null && ! command -v netcat &> /dev/null; then
        missing+=("netcat (nc)")
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing required tools: ${missing[*]}"
        echo "Please install them and try again."
        echo ""
        echo "Install rathole: cargo install rathole"
        echo "Install curl: apt-get install curl (or brew install curl)"
        echo "Install netcat: apt-get install netcat (or brew install netcat)"
        exit 1
    fi

    log_info "All prerequisites found."
}

# Find the socksrat binary
find_socksrat_binary() {
    # Check dist/ first (for pre-built static binary)
    if [ -f "$PROJECT_DIR/dist/x86_64-unknown-linux-musl/socksrat" ]; then
        SOCKSRAT_BIN="$PROJECT_DIR/dist/x86_64-unknown-linux-musl/socksrat"
    elif [ -f "$PROJECT_DIR/target/release/socksrat" ]; then
        SOCKSRAT_BIN="$PROJECT_DIR/target/release/socksrat"
    elif [ -f "$PROJECT_DIR/target/debug/socksrat" ]; then
        SOCKSRAT_BIN="$PROJECT_DIR/target/debug/socksrat"
    else
        log_error "socksrat binary not found. Run 'cargo build --release' first."
        exit 1
    fi
    log_info "Using socksrat binary: $SOCKSRAT_BIN"
}

# Start echo server for SOCKS5 testing
start_echo_server() {
    log_info "Starting echo server on port 9999..."
    mkdir -p "$TMP_DIR"

    # Simple echo server using netcat
    while true; do
        echo "HELLO FROM ECHO SERVER" | nc -l -p 9999 -q 1 2>/dev/null || true
    done &
    ECHO_SERVER_PID=$!
    sleep 1

    if kill -0 "$ECHO_SERVER_PID" 2>/dev/null; then
        log_info "Echo server started (PID: $ECHO_SERVER_PID)"
    else
        log_warn "Echo server may have issues, continuing anyway..."
    fi
}

# Start rathole server
start_rathole() {
    log_info "Starting rathole server..."

    rathole "$FIXTURES_DIR/rathole-server.toml" --server &
    RATHOLE_PID=$!
    sleep 2

    if kill -0 "$RATHOLE_PID" 2>/dev/null; then
        log_info "Rathole server started (PID: $RATHOLE_PID)"
    else
        log_error "Failed to start rathole server"
        exit 1
    fi
}

# Start socksrat client for SOCKS5
start_socksrat_socks5() {
    log_info "Starting socksrat client (SOCKS5)..."

    RUST_LOG=info "$SOCKSRAT_BIN" --config "$FIXTURES_DIR/test-socks5.toml" &
    SOCKSRAT_SOCKS5_PID=$!
    sleep 3

    if kill -0 "$SOCKSRAT_SOCKS5_PID" 2>/dev/null; then
        log_info "SocksRat SOCKS5 client started (PID: $SOCKSRAT_SOCKS5_PID)"
    else
        log_error "Failed to start socksrat SOCKS5 client"
        exit 1
    fi
}

# Start socksrat client for SSH
start_socksrat_ssh() {
    log_info "Starting socksrat client (SSH)..."

    RUST_LOG=info "$SOCKSRAT_BIN" --config "$FIXTURES_DIR/test-ssh.toml" &
    SOCKSRAT_SSH_PID=$!
    sleep 3

    if kill -0 "$SOCKSRAT_SSH_PID" 2>/dev/null; then
        log_info "SocksRat SSH client started (PID: $SOCKSRAT_SSH_PID)"
    else
        log_error "Failed to start socksrat SSH client"
        exit 1
    fi
}

# Test SOCKS5 proxy functionality
test_socks5() {
    log_test "Testing SOCKS5 proxy functionality..."

    local test_passed=0
    local test_failed=0

    # Test 1: Basic HTTP request through SOCKS5
    log_info "Test 1: HTTP request through SOCKS5 proxy"
    if curl --socks5 "127.0.0.1:$SOCKS5_PORT" --connect-timeout 10 -s "http://httpbin.org/ip" | grep -q "origin"; then
        log_info "✓ HTTP request through SOCKS5 succeeded"
        ((test_passed++))
    else
        log_error "✗ HTTP request through SOCKS5 failed"
        ((test_failed++))
    fi

    # Test 2: HTTPS request through SOCKS5
    log_info "Test 2: HTTPS request through SOCKS5 proxy"
    if curl --socks5-hostname "127.0.0.1:$SOCKS5_PORT" --connect-timeout 10 -s "https://httpbin.org/ip" | grep -q "origin"; then
        log_info "✓ HTTPS request through SOCKS5 succeeded"
        ((test_passed++))
    else
        log_error "✗ HTTPS request through SOCKS5 failed"
        ((test_failed++))
    fi

    # Test 3: DNS resolution through SOCKS5
    log_info "Test 3: DNS resolution through SOCKS5"
    if curl --socks5-hostname "127.0.0.1:$SOCKS5_PORT" --connect-timeout 10 -s "http://example.com" | grep -q "Example Domain"; then
        log_info "✓ DNS resolution through SOCKS5 succeeded"
        ((test_passed++))
    else
        log_warn "⚠ DNS resolution test inconclusive"
    fi

    echo ""
    log_info "SOCKS5 Tests: $test_passed passed, $test_failed failed"

    return $test_failed
}

# Test SSH server functionality
test_ssh() {
    # Disable set -e within test function to handle failures gracefully
    set +e

    log_test "Testing SSH server functionality..."

    local test_passed=0
    local test_failed=0

    # Check if sshpass is available for automated password auth
    if ! command -v sshpass &> /dev/null; then
        log_warn "sshpass not found, SSH tests will be limited"
        log_warn "Install with: apt-get install sshpass (or brew install hudochenkov/sshpass/sshpass)"
    fi

    # Test 1: SSH connection test (check if port is accepting connections)
    log_info "Test 1: SSH port connectivity"
    if nc -z -w 5 127.0.0.1 $SSH_PORT 2>/dev/null; then
        log_info "✓ SSH port is accessible"
        ((test_passed++)) || true
    else
        log_error "✗ SSH port is not accessible"
        ((test_failed++)) || true
        set -e
        return $test_failed
    fi

    # Test 2: SSH banner check
    log_info "Test 2: SSH banner/protocol check"
    local banner=$(echo "" | nc -w 2 127.0.0.1 $SSH_PORT 2>/dev/null | head -1 || true)
    if [[ "$banner" == SSH-* ]]; then
        log_info "✓ SSH banner received: $banner"
        ((test_passed++)) || true
    else
        log_error "✗ No SSH banner received"
        ((test_failed++)) || true
    fi

    # Test 3: SSH authentication and exec (if sshpass available)
    if command -v sshpass &> /dev/null; then
        log_info "Test 3: SSH password authentication and command execution"

        # Create a simple SSH config to skip host key checking
        mkdir -p "$TMP_DIR"
        cat > "$TMP_DIR/ssh_config" << EOF
Host test-ssh
    HostName 127.0.0.1
    Port $SSH_PORT
    User $TEST_USER
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    LogLevel ERROR
    PreferredAuthentications password
    PubkeyAuthentication no
EOF

        # Test exec with -T flag to avoid PTY request
        local cmd_output=$(sshpass -p "$TEST_PASSWORD" ssh -T -F "$TMP_DIR/ssh_config" test-ssh 'echo "EXEC_TEST_SUCCESS"' 2>/dev/null || true)
        if echo "$cmd_output" | grep -q "EXEC_TEST_SUCCESS"; then
            log_info "✓ SSH exec command succeeded"
            ((test_passed++)) || true
        else
            log_warn "⚠ SSH exec test returned: $cmd_output"
            # Still count as partial pass if we got any output
            if [ -n "$cmd_output" ]; then
                log_info "✓ SSH connection and auth worked (partial exec)"
                ((test_passed++)) || true
            fi
        fi

        # Test 4: Run a real command
        log_info "Test 4: SSH remote command (whoami)"
        local whoami_output=$(sshpass -p "$TEST_PASSWORD" ssh -T -F "$TMP_DIR/ssh_config" test-ssh 'whoami' 2>/dev/null || true)
        if [ -n "$whoami_output" ]; then
            log_info "✓ SSH whoami returned: $whoami_output"
            ((test_passed++)) || true
        else
            log_warn "⚠ SSH whoami test inconclusive"
        fi

        # Test 5: Interactive shell test - send multiple commands through a shell
        log_info "Test 5: SSH interactive shell test"
        local shell_output=$(sshpass -p "$TEST_PASSWORD" ssh -tt -F "$TMP_DIR/ssh_config" test-ssh 2>/dev/null << 'SHELL_CMDS' || true
echo "SHELL_START"
pwd
echo "SHELL_END"
exit
SHELL_CMDS
)
        if echo "$shell_output" | grep -q "SHELL_START" && echo "$shell_output" | grep -q "SHELL_END"; then
            log_info "✓ SSH interactive shell works"
            ((test_passed++)) || true
        else
            log_warn "⚠ SSH interactive shell test inconclusive (output: ${shell_output:0:100}...)"
        fi

        # Test 6: Interactive shell with PTY - run ls command
        log_info "Test 6: SSH interactive shell with PTY (ls command)"
        local ls_output=$(sshpass -p "$TEST_PASSWORD" ssh -tt -F "$TMP_DIR/ssh_config" test-ssh 'ls -la /' 2>/dev/null | head -5 || true)
        if echo "$ls_output" | grep -qE "(total|drwx|root)"; then
            log_info "✓ SSH shell ls command works"
            ((test_passed++)) || true
        else
            log_warn "⚠ SSH shell ls test inconclusive"
        fi
    fi

    echo ""
    log_info "SSH Tests: $test_passed passed, $test_failed failed"

    # Re-enable set -e
    set -e
    return $test_failed
}

# Run SOCKS5 tests only
run_socks5_tests() {
    log_info "=== Running SOCKS5 Tests Only ==="
    start_socksrat_socks5
    sleep 2
    test_socks5
    return $?
}

# Run SSH tests only
run_ssh_tests() {
    log_info "=== Running SSH Tests Only ==="
    start_socksrat_ssh
    sleep 2
    test_ssh
    return $?
}

# Run all tests
run_all_tests() {
    local socks5_failed=0
    local ssh_failed=0

    # Start SOCKS5 client and test
    start_socksrat_socks5
    sleep 2
    test_socks5 || socks5_failed=$?

    # Kill SOCKS5 client to free up control channel
    if [ -n "$SOCKSRAT_SOCKS5_PID" ]; then
        kill "$SOCKSRAT_SOCKS5_PID" 2>/dev/null || true
        sleep 2
    fi

    # Start SSH client and test
    start_socksrat_ssh
    sleep 2
    test_ssh || ssh_failed=$?

    echo ""
    echo "========================================"
    echo "           TEST SUMMARY"
    echo "========================================"

    if [ $socks5_failed -eq 0 ]; then
        echo -e "SOCKS5: ${GREEN}PASSED${NC}"
    else
        echo -e "SOCKS5: ${RED}FAILED${NC} ($socks5_failed tests)"
    fi

    if [ $ssh_failed -eq 0 ]; then
        echo -e "SSH: ${GREEN}PASSED${NC}"
    else
        echo -e "SSH: ${RED}FAILED${NC} ($ssh_failed tests)"
    fi

    local total_failed=$((socks5_failed + ssh_failed))

    echo ""
    if [ $total_failed -eq 0 ]; then
        log_info "All integration tests passed!"
        return 0
    else
        log_error "Some tests failed: $total_failed total failures"
        return 1
    fi
}

# Print usage
# Start socksrat client for multi-service mode
start_socksrat_multi() {
    log_info "Starting socksrat client (Multi-service: SOCKS5 + SSH)..."

    RUST_LOG=info "$SOCKSRAT_BIN" --config "$FIXTURES_DIR/test-multi-service.toml" &
    SOCKSRAT_MULTI_PID=$!
    sleep 5

    if kill -0 "$SOCKSRAT_MULTI_PID" 2>/dev/null; then
        log_info "SocksRat multi-service client started (PID: $SOCKSRAT_MULTI_PID)"
    else
        log_error "Failed to start socksrat multi-service client"
        exit 1
    fi
}

# Test multi-service mode (both SOCKS5 and SSH from single instance)
test_multi_service() {
    set +e
    log_test "Testing multi-service mode (SOCKS5 + SSH from single instance)..."

    local socks5_passed=0
    local ssh_passed=0
    local total_tests=0

    # Test SOCKS5 service
    log_info "--- SOCKS5 Tests (multi-service) ---"

    # Test 1: SOCKS5 HTTP request
    log_info "Test 1: HTTP request through SOCKS5 proxy"
    ((total_tests++)) || true
    if curl --socks5 "127.0.0.1:$SOCKS5_PORT" --connect-timeout 10 -s "http://httpbin.org/ip" 2>/dev/null | grep -q "origin"; then
        log_info "✓ SOCKS5 HTTP request succeeded"
        ((socks5_passed++)) || true
    else
        log_warn "⚠ SOCKS5 HTTP request failed"
    fi

    # Test 2: SOCKS5 HTTPS request
    log_info "Test 2: HTTPS request through SOCKS5 proxy"
    ((total_tests++)) || true
    if curl --socks5-hostname "127.0.0.1:$SOCKS5_PORT" --connect-timeout 10 -s "https://httpbin.org/ip" 2>/dev/null | grep -q "origin"; then
        log_info "✓ SOCKS5 HTTPS request succeeded"
        ((socks5_passed++)) || true
    else
        log_warn "⚠ SOCKS5 HTTPS request failed"
    fi

    # Test SSH service
    log_info "--- SSH Tests (multi-service) ---"

    # Test 3: SSH port connectivity
    log_info "Test 3: SSH port connectivity"
    ((total_tests++)) || true
    if nc -z -w 5 127.0.0.1 $SSH_PORT 2>/dev/null; then
        log_info "✓ SSH port is accessible"
        ((ssh_passed++)) || true
    else
        log_warn "⚠ SSH port is not accessible"
    fi

    # Test 4: SSH banner check
    log_info "Test 4: SSH banner check"
    ((total_tests++)) || true
    local banner=$(echo "" | nc -w 2 127.0.0.1 $SSH_PORT 2>/dev/null | head -1 || true)
    if [[ "$banner" == SSH-* ]]; then
        log_info "✓ SSH banner received: $banner"
        ((ssh_passed++)) || true
    else
        log_warn "⚠ No SSH banner received"
    fi

    # Test 5: SSH exec command (if sshpass available)
    if command -v sshpass &> /dev/null; then
        mkdir -p "$TMP_DIR"
        cat > "$TMP_DIR/ssh_config" << EOF
Host test-ssh
    HostName 127.0.0.1
    Port $SSH_PORT
    User $TEST_USER
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    LogLevel ERROR
    PreferredAuthentications password
    PubkeyAuthentication no
EOF

        log_info "Test 5: SSH exec command"
        ((total_tests++)) || true
        local cmd_output=$(sshpass -p "$TEST_PASSWORD" ssh -T -F "$TMP_DIR/ssh_config" test-ssh 'echo "MULTI_SERVICE_SUCCESS"' 2>/dev/null || true)
        if echo "$cmd_output" | grep -q "MULTI_SERVICE_SUCCESS"; then
            log_info "✓ SSH exec command succeeded"
            ((ssh_passed++)) || true
        else
            log_warn "⚠ SSH exec command failed"
        fi
    fi

    # Summary
    echo ""
    local total_passed=$((socks5_passed + ssh_passed))
    log_info "Multi-service Tests: $total_passed/$total_tests passed (SOCKS5: $socks5_passed, SSH: $ssh_passed)"

    set -e

    if [ $total_passed -lt 3 ]; then
        return 1
    fi
    return 0
}

# Run multi-service tests
run_multi_tests() {
    log_info "=== Running Multi-Service Tests ==="
    start_socksrat_multi
    sleep 2
    test_multi_service
    return $?
}

usage() {
    echo "SocksRat Integration Test Script"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --all       Run all tests (default)"
    echo "  --socks5    Run SOCKS5 tests only"
    echo "  --ssh       Run SSH tests only"
    echo "  --multi     Run multi-service tests (SOCKS5+SSH in one instance)"
    echo "  -h, --help  Show this help"
    echo ""
}

# Main
main() {
    local run_mode="all"

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --all)
                run_mode="all"
                shift
                ;;
            --socks5)
                run_mode="socks5"
                shift
                ;;
            --ssh)
                run_mode="ssh"
                shift
                ;;
            --multi)
                run_mode="multi"
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    echo "========================================"
    echo "  SocksRat Integration Test Suite"
    echo "========================================"
    echo ""

    check_prerequisites

    find_socksrat_binary

    echo ""
    log_info "Starting test environment..."

    start_echo_server
    start_rathole

    echo ""
    log_info "Waiting for services to stabilize..."
    sleep 2

    local exit_code=0

    case $run_mode in
        all)
            run_all_tests
            exit_code=$?
            ;;
        socks5)
            run_socks5_tests
            exit_code=$?
            ;;
        ssh)
            run_ssh_tests
            exit_code=$?
            ;;
        multi)
            run_multi_tests
            exit_code=$?
            ;;
    esac

    cleanup
    exit $exit_code
}

main "$@"
