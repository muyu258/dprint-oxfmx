import { format } from "oxfmt";

const input = await readInput();
const request = JSON.parse(input) as {
  fileName: string;
  sourceText: string;
  options: Record<string, unknown>;
};
const result = await format(request.fileName, request.sourceText, request.options);
process.stdout.write(JSON.stringify(result));

async function readInput(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString("utf8");
}
