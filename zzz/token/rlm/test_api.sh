#!/bin/bash

echo "Testing Groq-compatible API with curl..."
echo ""

API_KEY="${RLM_API_KEY:-$GROQ_API_KEY}"
if [ -z "$API_KEY" ]; then
  echo "Set RLM_API_KEY or GROQ_API_KEY before running this script."
  exit 1
fi

curl -s https://api.groq.com/openai/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $API_KEY" \
  -H "User-Agent: RLM Smoke Test" \
  -d '{
    "model": "llama-3.3-70b-versatile",
    "messages": [
      {
        "role": "user",
        "content": "What is 2+2? Answer in one word."
      }
    ],
    "max_tokens": 10
  }' | python3 -m json.tool

echo ""
echo "If you see a response above, the provider is reachable."
