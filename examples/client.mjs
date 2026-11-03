// Install the client dependency: npm install openai
import OpenAI from 'openai';
const client = new OpenAI({
  baseURL: process.env.OPENAI_BASE_URL ?? 'http://127.0.0.1:8787/v1',
  apiKey: process.env.OPENAI_API_KEY,
});
const stream = await client.chat.completions.create({
  model: process.env.HARNESS_MODEL ?? 'codex/default',
  messages: [{ role: 'user', content: 'Explain reverse proxies in one sentence.' }],
  stream: true,
});
for await (const chunk of stream) process.stdout.write(chunk.choices[0]?.delta.content ?? '');
process.stdout.write('\n');
