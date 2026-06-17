FROM docker:24-dind

RUN apk add --no-cache bash curl git python3 docker-cli-compose

COPY deployment/runpod/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
