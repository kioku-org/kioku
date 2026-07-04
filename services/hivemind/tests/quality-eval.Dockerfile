# Reproducible runner for the gold-set retrieval quality eval (quality_retrieval.rs, #71).
#
# Build once:
#   docker build -f services/hivemind/tests/quality-eval.Dockerfile -t hivemind-quality-eval services/hivemind/tests
#
# Run against the all-in-one stateful container (shares its network namespace, so
# localhost:9100/localhost:11434 resolve to the same ports the container itself uses):
#   docker run --rm --network container:kioku-stateful \
#     -e HIVEMIND_URL=http://localhost:9100 \
#     -e EMBEDDING_API_URL=http://localhost:11434 \
#     hivemind-quality-eval
#
# Run against a docker-compose multi-service deployment instead (adjust hostnames/network):
#   docker run --rm --network <compose-network> \
#     -e HIVEMIND_URL=http://hivemind:9100 \
#     -e EMBEDDING_API_URL=http://ollama:11434 \
#     hivemind-quality-eval
FROM rust:1.88-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /tests
COPY . .
RUN cargo build --test quality_retrieval
CMD ["cargo", "test", "--test", "quality_retrieval", "--", "--nocapture"]
