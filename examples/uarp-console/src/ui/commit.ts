// Commits the turn's view operations once, at the end of the turn's synchronous
// work (in its microtask checkpoint). View event turns commit by themselves, and
// a commit with nothing pending does nothing, so calling this from any turn is safe.
let scheduled = false;

export function commitSoon(): void {
  if (scheduled) return;
  scheduled = true;
  Promise.resolve().then((): void => {
    scheduled = false;
    View.commit();
  });
}
