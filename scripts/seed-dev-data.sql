-- Ops Local Development Seed Data
-- Run via: psql -U ops -d ops -f seed-dev-data.sql
-- Idempotent: only inserts into empty tables.

-- Glossary: application-level terms only
INSERT INTO glossary (term, full_name, description, aliases)
VALUES
  ('KBP', 'Kolya BR Proxy', 'AI Gateway providing unified OpenAI/Anthropic API access to AWS Bedrock and Google Gemini, with prompt caching for up to 90% cost savings', ARRAY['kbr','br-proxy'])
ON CONFLICT DO NOTHING;

-- Knowledge base
INSERT INTO knowledge_files (filename, content, size_bytes, mime_type)
SELECT 'kbp-architecture.md', content, LENGTH(content), 'text/markdown'
FROM (VALUES (
'# Kolya BR Proxy (KBP) Architecture

## Overview

KBP is a production-grade AI Gateway providing unified OpenAI-compatible and Anthropic Messages API access to multiple cloud LLM providers. Core value: **cost optimization (up to 90% savings via prompt caching)**, **multi-cloud model routing**, and **enterprise security**.

## API Endpoints

- `POST /v1/chat/completions` — OpenAI-compatible endpoint
- `POST /v1/messages` — Anthropic Messages API endpoint

Both route to the same backend; clients only change `base_url` and `api_key`.

## Backend Providers

| Provider | API | Models |
|----------|-----|--------|
| AWS Bedrock (InvokeModel) | Native Anthropic format | Claude family |
| AWS Bedrock (Converse) | Model-agnostic | Nova, DeepSeek, Mistral, Llama, 19+ others |
| Google Gemini | Native generateContent | Gemini family |

## Prompt Caching

Auto-injects `cache_control` markers (up to 4) into stable content. Priority: tool definitions > system prompt > assistant messages. Cached reads billed at 0.1x input token price. ~60% savings in multi-turn agent loops.

## Infrastructure

- **Compute**: EKS + Karpenter (ARM64 Graviton), HPA 1-10 replicas
- **Database**: Aurora PostgreSQL (AsyncPG)
- **Cache**: Redis (distributed token bucket rate limiting)
- **Secrets**: AWS Secrets Manager → ESO → K8s Secrets
- **Security**: WAF (SQLi/XSS) + ALB + CORS/CSRF triple-layer
- **Auth**: JWT (dashboard, 30min TTL) + API Keys (gateway, `kbr_*` prefix, SHA256 hashed)

## Request Flow

```
Client → ALB/WAF → FastAPI Router → RequestTranslator → BedrockClient/GeminiClient → LLM Provider
                                                      ↑ prompt caching injection
                                                      ↑ rate limiting (Redis token bucket)
```

## Key Design Decisions

1. **Dual API compatibility** — Accept OpenAI + Anthropic formats, normalize to internal schema
2. **Inference Profile Cache** — Cache available models from AWS at startup + daily 03:00 UTC refresh
3. **Background async usage recording** — Defer DB writes to avoid blocking API responses
4. **Semaphore back-pressure** — asyncio.Semaphore(50) prevents request queue buildup
5. **Streaming heartbeat** — 15s empty SSE comments prevent ALB timeout (3600s default)
')) AS v(content)
WHERE NOT EXISTS (SELECT 1 FROM knowledge_files LIMIT 1);

-- Cloud accounts
INSERT INTO cloud_accounts (provider, name, account_id, config, is_mock)
SELECT * FROM (VALUES
  ('aws',      'AWS Production',         '612674025488', '{"region":"us-east-1"}'::jsonb, false),
  ('alicloud', 'Alicloud China (Mock)',   'ali-mock-001', '{"mode":"mock"}'::jsonb,       true),
  ('azure',    'Azure Global (Mock)',     'sub-mock-001', '{"mode":"mock"}'::jsonb,       true)
) AS v(provider, name, account_id, config, is_mock)
WHERE NOT EXISTS (SELECT 1 FROM cloud_accounts LIMIT 1);

-- Telemetry config (Grafana Cloud endpoints — API token left blank for security)
INSERT INTO telemetry_config (provider, config, enabled)
SELECT 'grafana', '{
  "api_token": "",
  "loki_user_id": "1535497",
  "loki_endpoint_url": "https://logs-prod-030.grafana.net",
  "tempo_user_id": "1529802",
  "tempo_endpoint_url": "https://tempo-prod-20-prod-ap-northeast-0.grafana.net/tempo",
  "mimir_user_id": "3079644",
  "mimir_endpoint_url": "https://prometheus-prod-49-prod-ap-northeast-0.grafana.net/api/prom"
}'::jsonb, false
WHERE NOT EXISTS (SELECT 1 FROM telemetry_config LIMIT 1);

-- Issues: no seed data — real issues come from Grafana webhooks via POST /api/alerts
