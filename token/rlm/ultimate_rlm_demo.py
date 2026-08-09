#!/usr/bin/env python3
"""
Standalone long-context demo for Groq-compatible chat completions.
"""

import json
import os
import time
import urllib.error
import urllib.request

GROQ_API_KEY = os.environ.get("RLM_API_KEY") or os.environ.get("GROQ_API_KEY")
GROQ_API_URL = "https://api.groq.com/openai/v1/chat/completions"
MODEL = "llama-3.3-70b-versatile"


def call_groq_api(messages, max_tokens=1024, retry_count=3):
    if not GROQ_API_KEY:
        raise RuntimeError("Set RLM_API_KEY or GROQ_API_KEY before running this demo.")

    headers = {
        "Authorization": f"Bearer {GROQ_API_KEY}",
        "Content-Type": "application/json",
        "User-Agent": "RLM Demo",
    }

    data = {
        "model": MODEL,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0.7,
    }

    for attempt in range(retry_count):
        request = urllib.request.Request(
            GROQ_API_URL,
            data=json.dumps(data).encode("utf-8"),
            headers=headers,
            method="POST",
        )

        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as error:
            if error.code == 429 and attempt + 1 < retry_count:
                time.sleep(2 ** attempt)
                continue
            raise


def build_demo_document():
    parts = [
        "# Technology Industry Report 2024",
        "AI market: $184 billion in 2024, 37.3% annual growth.",
        "SpaceX launches: 96 successful launches in 2024.",
        "Remote work: 42% of tech workers fully remote in 2024.",
    ]
    for index in range(1, 40):
        parts.append(
            f"Section {index}: synthetic filler content for long-context experiments about topic {index}."
        )
    return "\n\n".join(parts)


def main():
    document = build_demo_document()
    prompt = (
        "Search this document and answer: What is the AI market size in 2024?\n\n"
        + document[:6000]
    )

    started = time.time()
    result = call_groq_api([{"role": "user", "content": prompt}], max_tokens=300)
    elapsed = time.time() - started

    answer = result["choices"][0]["message"]["content"]
    print("Elapsed: {:.2f}s".format(elapsed))
    print("Answer:")
    print(answer)


if __name__ == "__main__":
    main()
