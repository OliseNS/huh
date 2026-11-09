# Run:
#  uv run --with openai idk.py "your prompt here"
# or
#  echo "your prompt here" | uv run --with openai idk.py

import sys
from enum import StrEnum

from openai import OpenAI

client = OpenAI(
    base_url="https://llm.codegig.xyz",
    api_key="sk-yadayada",
)

class ThinkingEffort(StrEnum):
    LOW = 'low'
    MEDIUM = 'medium'
    HIGH = 'high'

EFFORT = ThinkingEffort.LOW

def get_prompt():
    if len(sys.argv) > 1:
        return " ".join(sys.argv[1:])
    if not sys.stdin.isatty():
        return sys.stdin.read().strip()
    return ""

prompt = get_prompt()
if not prompt:
    print(f"USAGE ERR: {sys.argv[0]} <prompt>  (or pipe prompt on stdin)", file=sys.stderr)
    sys.exit(1)

stream = client.responses.create(
    model='gpt-oss-20b',
    instructions="You are printing output to a bash command `idk <topic>`. Be effective in your answer be with as little content needed to completely definition the topic in question. Make the length of your response symmetric to the simplicity of the question.",
    # instructions="You are printing output to a bash command `howto <topic>`. Be effective in your answer and provide steps with instructions and explanations to make sure the user knows how to complete the task on their own. Make the length of your response symmetric to the simplicity of the question",
    input=prompt,
    stream=True,
    store=False,
    reasoning={"effort": EFFORT}
)

try:
    for event in stream:
        if event.type == "response.output_text.delta":
            print(event.delta, end="", flush=True)
        elif event.type == "error":
            print(f"\nERROR: {event.message}", file=sys.stderr)
finally:
    stream.close()
print()
