# Toy Blockchain - Build and Usage Guide

This project is a functional toy blockchain implemented from scratch in Rust. It features a UTXO model, Proof-of-Work consensus, persistence via `sled`, a P2P networking layer, and a REST API.

## Prerequisites

- **Rust**: Ensure you have the latest stable version of Rust installed.
- **Cargo**: The Rust package manager (included with Rust).

## Building the Project

To build the project, navigate to the project root and run:

```bash
cargo build --release
```

The compiled binary will be located at `target/release/toy_blockchain`.

## Running the Node

To start a blockchain node:

```bash
cargo run
```

By default, the node will:
- Initialize or open the `blockchain_db` (using `sled`).
- Start a **P2P Server** on `127.0.0.1:9000`.
- Start a **REST API** on `127.0.0.1:3000`.

## REST API Endpoints

### 1. View Blockchain
**Endpoint**: `GET /blocks`
Returns the full chain of blocks in JSON format.

```bash
curl http://127.0.0.1:3000/blocks
```

### 2. Check Balance
**Endpoint**: `GET /balance/:address`
Returns the current balance for a given base64-encoded public key.

```bash
curl http://127.0.0.1:3000/balance/<base64_address>
```

### 3. Submit Transaction
**Endpoint**: `POST /transactions`
Submits a new transaction to the mempool.

```bash
curl -X POST http://127.0.0.1:3000/transactions \
     -H "Content-Type: application/json" \
     -d '{
       "inputs": [...],
       "outputs": [...],
       "timestamp": 123456789
     }'
```

## Key Features Implemented

- **UTXO Model**: Transactions use inputs and outputs instead of simple account balances.
- **Persistence**: The blockchain state is saved to disk using the `sled` key-value store.
- **Networking**: A basic P2P layer using `tokio` and `TcpStream` for node communication.
- **Merkle Trees**: Each block contains a Merkle root of its transactions for efficient verification.
- **Consensus**: Implements Proof-of-Work with a configurable difficulty target.
- **Error Handling**: Robust error management using `thiserror` and `anyhow`.

## Project Structure

- `src/main.rs`: Contains the entire implementation including data structures, consensus logic, networking, and API handlers.
- `Cargo.toml`: Project dependencies and configuration.
- `blockchain_db/`: Directory where the blockchain state is persisted.
