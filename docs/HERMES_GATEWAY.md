# Hermes Gateway Blueprint

The Hermes buddy talks to the local desktop gateway over:

```text
ws://127.0.0.1:17387/border-buddies
```

The gateway is intentionally small. It owns provider credentials and sends Hermes chat through an OpenAI-compatible `/chat/completions` endpoint. The desktop buddy only knows the local WebSocket URL.

## Configure

Copy the example and fill in the private provider values locally:

```bash
cp .env.example .env
```

Required for a real provider:

```bash
HERMES_PROVIDER=xai
HERMES_API_BASE=https://api.x.ai/v1
HERMES_API_KEY=...
HERMES_MODEL=...
```

Local model servers work too if they expose the OpenAI-compatible chat route:

```bash
HERMES_PROVIDER=lm_studio
HERMES_API_BASE=http://127.0.0.1:1234/v1
HERMES_MODEL=<loaded-model-name>
```

If `HERMES_PROVIDER=echo`, or if base/model is missing, the gateway stays in fallback echo mode. That proves buddy-to-gateway wiring but not a real Hermes provider response.

## Start

Gateway only:

```bash
bash scripts/bb-gateway.sh
```

Desktop plus gateway:

```bash
bash scripts/bb-start.sh
```

Both scripts source `.env` before starting, so restart the gateway after changing provider settings.

## Handshake

The desktop sends:

```json
{ "type": "hello", "source": "border-dock", "version": 1 }
```

The gateway replies with status:

```json
{
  "type": "status",
  "gateway": "hermes-gateway",
  "provider": "xai",
  "buddies": ["hermes"],
  "message": "Hermes gateway ready (xai: <model>)"
}
```

Hermes chat sends:

```json
{
  "type": "chat",
  "buddy": "hermes",
  "text": "hello",
  "requestId": "req-..."
}
```

The gateway replies:

```json
{
  "type": "chat_reply",
  "buddy": "hermes",
  "text": "...",
  "requestId": "req-..."
}
```

This is the first adapter pattern for other buddies: keep credentials and provider-specific behavior in the gateway, keep the desktop buddy on the same local WebSocket protocol, and add provider metadata to the status/reply logs.
