// Commits the view operations of changes made outside a view event's handler
// (timers, request completions, the continuations of awaited requests). View
// event turns commit by themselves, right after the handler and before its
// microtasks, and the engine allows one commit per turn, ending the instance on
// a second. So the commit runs in a zero-delay timer turn of its own, never in
// the turn that changed something: a handler's awaited continuation (a history
// load that finishes in the microtasks of its click) would otherwise commit a
// second time in that turn. Changes made before the timer fires are committed
// together; a commit with nothing pending does nothing.
let scheduled = false;

function flush(): void {
  scheduled = false;
  View.commit();
}

export function commitSoon(): void {
  if (scheduled) return;
  scheduled = true;
  setTimeout(flush, 0);
}
