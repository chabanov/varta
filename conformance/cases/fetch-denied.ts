// A global fetch without a `net.request` grant rejects with a recoverable
// NotAllowedError DOMException before any socket is opened (the runner grants
// nothing), and the program continues; a relative URL is a TypeError.
export async function main(): Promise<void> {
  try {
    await fetch('http://127.0.0.1:9/never');
    console.log('fetched');
  } catch (error) {
    const name = error instanceof DOMException ? error.name : 'other';
    console.log(`denied ${name}`);
  }
  try {
    await fetch('/relative');
  } catch (error) {
    console.log(`relative ${error instanceof TypeError}`);
  }
  console.log('continued');
}
