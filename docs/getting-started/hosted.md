---
title: "For Hosted Users"
description: "Start transcribing meetings on kioku.chat in minutes."
---

## Sign In

Go to [dashboard.kioku.chat](https://dashboard.kioku.chat) and sign in with your email or Google account.

## Join a Meeting

1. Open the **Meetings** tab
2. Paste a Google Meet, Zoom, or Microsoft Teams URL
3. Click **Join Meeting**

The Kioku bot enters the meeting, captures audio, and transcribes in real time. Once the meeting ends (or you click **Stop**), the transcript is indexed and becomes searchable.

<Note>
  Leave **Authenticated** toggle OFF. Authenticated mode requires pre-stored browser cookies and is not yet supported in the hosted product. The bot joins via the Ask to Join waiting room.
</Note>

## Search Your Meetings

Use the **Search** bar to query across all your transcripts and uploaded documents using semantic similarity — not keyword matching.

## Upload Documents

Drag a PDF onto the **Documents** panel to add it to your knowledge base. The text is extracted, embedded, and becomes searchable alongside your meeting transcripts.

## Connect an AI Client (MCP)

To let Claude, Cursor, or another MCP client access your knowledge base:

```bash
kioku mcp
```

This prints a ready-to-paste JSON config with both MCP servers and your current token. See [MCP / Cursor / Claude](/getting-started/mcp-cursor-claude) for details.

## API Access

Get your API key from **Settings → API Keys** in the dashboard. All REST endpoints are at `https://meetings.kioku.chat`.

```bash
curl -H "Authorization: Bearer YOUR_KEY" https://meetings.kioku.chat/bots
```
