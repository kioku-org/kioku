FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update -qq && apt-get install -y -qq curl wget gnupg lsb-release sudo tzdata
COPY entrypoint-stateful.sh /tmp/init.sh
RUN bash /tmp/init.sh
CMD ["/usr/bin/supervisord", "-c", "/etc/supervisor/conf.d/supervisord.conf"]
