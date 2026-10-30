// Abort listeners run in registration order when the controller aborts; a
// once listener and a removed listener run at most once and never.
export function main(): void {
  const controller = new AbortController();
  const order: string[] = [];
  const removed = (): void => {
    order.push('removed');
  };
  controller.signal.addEventListener('abort', (): void => {
    order.push(`first ${String(controller.signal.aborted)}`);
  });
  controller.signal.addEventListener(
    'abort',
    (): void => {
      order.push('once');
    },
    { once: true },
  );
  controller.signal.addEventListener('abort', removed);
  controller.signal.removeEventListener('abort', removed);
  controller.abort();
  controller.abort();
  console.log(JSON.stringify(order));
}
