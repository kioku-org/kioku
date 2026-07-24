import test from 'node:test';
import assert from 'node:assert/strict';
import { GmeetChannelBinding } from '@vexa/gmeet-capture';
import { GmeetSpeakerRouter } from '../gmeet-speaker-routing';

test('confirmed channel names survive three-way overlap without switching', () => {
  const binding = new GmeetChannelBinding({ confirmFrames: 2, ownerHoldMs: 1500 });

  assert.equal(binding.resolve(0, ['Anna'], 0), undefined);
  assert.equal(binding.resolve(0, ['Anna'], 100), 'Anna');
  assert.equal(binding.resolve(1, ['Boris'], 200), undefined);
  assert.equal(binding.resolve(1, ['Boris'], 300), 'Boris');
  assert.equal(binding.resolve(2, ['Vera'], 400), undefined);
  assert.equal(binding.resolve(2, ['Vera'], 500), 'Vera');

  const overlap = ['Anna', 'Boris', 'Vera'];
  assert.equal(binding.resolve(0, overlap, 600), 'Anna');
  assert.equal(binding.resolve(1, overlap, 600), 'Boris');
  assert.equal(binding.resolve(2, overlap, 600), 'Vera');
});

test('ambiguous overlap never invents an initial speaker name', () => {
  const binding = new GmeetChannelBinding({ confirmFrames: 2 });
  const overlap = ['Anna', 'Boris', 'Vera'];

  assert.equal(binding.resolve(0, overlap, 0), undefined);
  assert.equal(binding.resolve(1, overlap, 0), undefined);
  assert.equal(binding.resolve(2, overlap, 0), undefined);
});

test('contradictory glow withholds a stale label and preserves one-name-per-channel', () => {
  const binding = new GmeetChannelBinding({ confirmFrames: 2, ownerHoldMs: 1000 });
  binding.resolve(0, ['Anna'], 0);
  assert.equal(binding.resolve(0, ['Anna'], 100), 'Anna');
  binding.resolve(1, ['Boris'], 200);
  assert.equal(binding.resolve(1, ['Boris'], 300), 'Boris');

  // Channel 0 suddenly correlates with Boris, but Boris's original channel is
  // still active. Never emit Anna for contradictory audio, and never duplicate Boris.
  assert.equal(binding.resolve(0, ['Boris'], 400), undefined);
  assert.equal(binding.resolve(0, ['Boris'], 500), undefined);
  assert.equal(binding.getBinding(0), 'Anna');
  assert.equal(binding.getBinding(1), 'Boris');

  // Once the old owner is stale, the already-confirmed candidate safely moves.
  assert.equal(binding.resolve(0, ['Boris'], 1500), 'Boris');
  assert.equal(binding.getBinding(1), undefined);
});

test('downstream routing follows stable names instead of rotating channels', () => {
  const router = new GmeetSpeakerRouter();
  const annaOnChannel0 = router.route(0, 'Anna');
  const annaOnChannel7 = router.route(7, 'Anna');
  const borisOnChannel0 = router.route(0, 'Boris');

  assert.equal(annaOnChannel0.speakerId, annaOnChannel7.speakerId);
  assert.notEqual(annaOnChannel0.speakerId, borisOnChannel0.speakerId);
  assert.deepEqual(router.route(3), { speakerId: 'gmeet-unknown-3', speakerName: '' });
});
