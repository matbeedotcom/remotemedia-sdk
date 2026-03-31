export function ResultDisplay({ result }: { result: any }) {
  if (!result) return null;

  // Handle different output types
  if (result.output) {
    const output = result.output;
    if (typeof output === 'string' || output.Text) {
      return <div class="result-text">{output.Text || output}</div>;
    }
    return <pre class="result-json">{JSON.stringify(output, null, 2)}</pre>;
  }

  return <pre class="result-json">{JSON.stringify(result, null, 2)}</pre>;
}
