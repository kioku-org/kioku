---
title: "Usage API"
---
Track token usage per user.

## Record Usage

<Endpoint method="POST" path="/usage" />

```json
{
    "session_id": "s-1",
    "model": "gpt-4o",
    "provider": "openai",
    "input_tokens": 5000,
    "output_tokens": 1200
}
```

## Usage Summary

<Endpoint method="GET" path="/usage/summary" />

Returns per-user usage breakdown:

```json
[
    {
        "user_id": "u-1",
        "email": "user@kioku.chat",
        "name": "User",
        "total_input_tokens": 50000,
        "total_output_tokens": 12000,
        "total_cost_cents": 42,
        "session_count": 15,
        "last_active_at": 1700000000
    }
]
```

### CLI Usage

```bash
kioku usage
# user@kioku.chat: 50000 in / 12000 out / $0.42
```