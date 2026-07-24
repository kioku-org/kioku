/**
 * Route source-bound Google Meet audio by participant identity, not by the
 * anonymous/rotating media channel that happened to carry the frame.
 */
export interface GmeetSpeakerRoute {
  speakerId: string;
  speakerName: string;
}

export class GmeetSpeakerRouter {
  private readonly idsByName = new Map<string, string>();
  private nextNamedId = 0;

  route(channel: number, capturedName?: string): GmeetSpeakerRoute {
    const name = capturedName?.trim() || '';
    if (!name) {
      return { speakerId: `gmeet-unknown-${channel}`, speakerName: '' };
    }

    let speakerId = this.idsByName.get(name);
    if (!speakerId) {
      speakerId = `gmeet-speaker-${this.nextNamedId++}`;
      this.idsByName.set(name, speakerId);
    }
    return { speakerId, speakerName: name };
  }

  reset(): void {
    this.idsByName.clear();
    this.nextNamedId = 0;
  }
}
