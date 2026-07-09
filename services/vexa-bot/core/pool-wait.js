'use strict';
// Blocks a warm-pool bot pod until runtime-api-runpod assigns it a real
// meeting. Deliberately plain CommonJS (no build step) so it can run via
// bare `node` before the rest of the bot's TypeScript entrypoint starts.
const { createClient } = require('redis');

const WAIT_TIMEOUT_SECONDS = 3600; // give up and exit after 1h unclaimed

async function main() {
  const url = process.env.POOL_REDIS_URL;
  const slotId = process.env.RUNPOD_POD_ID || process.env.POOL_SLOT_ID;
  if (!url || !slotId) {
    process.stderr.write('[pool-wait] POOL_REDIS_URL or slot id missing\n');
    process.exit(1);
  }

  const client = createClient({ url });
  client.on('error', (err) => {
    process.stderr.write(`[pool-wait] redis error: ${err.message}\n`);
  });
  await client.connect();

  const key = `pool:assign:${slotId}`;
  process.stderr.write(`[pool-wait] idle, waiting on ${key}\n`);

  const result = await client.blPop(key, WAIT_TIMEOUT_SECONDS);
  await client.quit();

  if (!result) {
    process.stderr.write('[pool-wait] timed out waiting for assignment\n');
    process.exit(1);
  }
  process.stdout.write(result.element);
}

main().catch((err) => {
  process.stderr.write(`[pool-wait] fatal: ${err.message}\n`);
  process.exit(1);
});
