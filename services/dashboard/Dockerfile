# Build stage — context: services/dashboard/
FROM node:20-alpine AS builder

WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts

COPY src ./src
COPY public ./public
COPY scripts ./scripts
COPY next.config.ts tsconfig.json postcss.config.mjs eslint.config.mjs components.json ./

# KIOKU_API_URL is baked into Next.js rewrites at build time.
# The container must be on the same Docker network as vexa-api-gateway.
ARG KIOKU_API_URL=http://vexa-api-gateway:8000
ENV KIOKU_API_URL=$KIOKU_API_URL

# Version shown in the dashboard UI. Pass a real semver at build time if desired.
ARG NEXT_PUBLIC_VEXA_OSS_VERSION=0.1.0
ENV NEXT_PUBLIC_VEXA_OSS_VERSION=$NEXT_PUBLIC_VEXA_OSS_VERSION

RUN npm run build

# Production stage
FROM node:20-alpine AS runner

WORKDIR /app

ENV NODE_ENV=production

RUN addgroup --system --gid 1001 nodejs && \
    adduser --system --uid 1001 nextjs

COPY --from=builder /app/public ./public
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static

USER nextjs

EXPOSE 3001
ENV PORT=3001
ENV HOSTNAME="0.0.0.0"

CMD ["node", "server.js"]
