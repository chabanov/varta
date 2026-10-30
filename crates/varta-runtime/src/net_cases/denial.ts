// V37 recoverable denials: a request outside the grant rejects with a
// catchable NotAllowedError before any socket, and the program goes on. So
// does a request the grant's method list does not admit.
const BASE = '__BASE__';

export async function main(): Promise<void> {
  try {
    await fetch(BASE + '/hello');
  } catch (error) {
    const named = error instanceof DOMException ? error.name : 'other';
    console.log(`denied: ${named} ${error instanceof DOMException && error.message.indexOf('net.request') >= 0}`);
  }
  try {
    await fetch(BASE + '/echo', { method: 'DELETE' });
  } catch (error) {
    console.log(`method: ${error instanceof DOMException ? error.name : 'other'}`);
  }
  try {
    await fetch('/relative');
  } catch (error) {
    console.log(`invalid: ${error instanceof TypeError}`);
  }
  console.log('continued');
}
