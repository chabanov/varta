// Toast and confirm-dialog state (src/ui/notices.ts): a toast leaves by itself
// after its duration; a confirmation runs its action only on yes, once.
import { Runtime, untracked } from "../l1/reactive.js";
import { Confirmation, Toasts, type ConfirmRequest, type Toast } from "../ui/notices.js";
import { equal, type Report } from "./check.js";

/** A short toast duration, so the test waits milliseconds. */
const DURATION_MS = 5;
/** Longer than DURATION_MS: every toast has left by then. */
const AFTER_MS = 20;

function wait(ms: number): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(() => resolve(), ms);
  });
}

function texts(rt: Runtime, toasts: Toasts): string {
  return untracked<Toast[]>(rt, toasts.list.get)
    .map((toast: Toast): string => String(toast.id) + " " + toast.text)
    .join("; ");
}

export async function noticeTests(r: Report): Promise<void> {
  const rt = new Runtime();
  let changes = 0;
  const toasts = new Toasts(rt, DURATION_MS, (): void => {
    changes += 1;
  });
  toasts.show("перший");
  toasts.show("другий");
  equal(r, "notices: toasts stack in order", texts(rt, toasts), "1 перший; 2 другий");
  await wait(AFTER_MS);
  equal(r, "notices: toasts leave by themselves", texts(rt, toasts), "");
  equal(r, "notices: every change asks for a commit", String(changes), "4");

  const confirmation = new Confirmation(rt);
  let runs = 0;
  const request: ConfirmRequest = {
    title: "Забути ключ API?",
    text: "",
    action: "Забути",
    run: (): void => {
      runs += 1;
    },
  };
  confirmation.ask(request);
  confirmation.answer(false);
  equal(r, "notices: no runs nothing", String(runs), "0");
  confirmation.ask(request);
  confirmation.answer(true);
  confirmation.answer(true);
  equal(r, "notices: yes runs the action once", String(runs), "1");
  equal(r, "notices: the dialog has no request after an answer", String(untracked<ConfirmRequest | null>(rt, confirmation.request.get) === null), "true");
}
