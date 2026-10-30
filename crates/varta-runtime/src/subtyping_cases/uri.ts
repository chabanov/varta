export function main(): void {
  const id = "a b/c?d=é&ü😀";
  console.log(`/api/v1/agents/${encodeURIComponent(String(id))}/schedule`);
  console.log(encodeURIComponent("safe-_.!~*'()AZaz09"));
  console.log(encodeURI("https://x.test/a b?q=1&r=é#frag"));
  console.log(encodeURIComponent(42));
  try {
    encodeURIComponent("\uD800");
  } catch (error) {
    console.log(error instanceof URIError ? `URIError ${error.message}` : "other");
  }
}
