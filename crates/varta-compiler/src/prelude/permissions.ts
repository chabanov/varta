// Engine prelude of checked-f64-utf16-v38 (`turn_budgets`): permission broker
// v1. The compiler adds it to a module that names `varta.permissions.query` or
// `varta.permissions.request` and binds them under those dotted names, which no
// source declaration can spell. See docs/architecture.md (Permission broker v1).
//
// The runtime's broker answers: `varta.permissions.state` returns the state
// code of a descriptor and never prompts; `varta.permissions.ask` returns the
// id of a one-shot host event source whose single record is the new state. It
// is ready at once, except in a user-gesture turn of an application instance
// with a prompt seam, where the answer arrives in a later turn. These
// intrinsics exist only in engine preludes.

// The descriptor and state codes of `varta_common::permission`; a compiler
// test keeps them in step.
const DESCRIPTOR_VERSION = 'varta-permission-1';
const SEPARATOR = '\n';
const NO_RESOURCE = '0';
const WITH_RESOURCE = '1';
const INVALID_DESCRIPTOR = 0;
const GRANTED = 1;
const DENIED = 2;
const PROMPT = 3;
// The record kind of a delivered answer (`varta_common::events::TakeKind`).
const KIND_EVENT = 1;

export type PermissionState = 'granted' | 'denied' | 'prompt';

export interface PermissionDescriptor {
  name: string;
  resource?: string;
}

export interface PermissionStatus {
  name: string;
  state: PermissionState;
}

// Suspend until the source `id` has a record.
function waitFor(id: number): Promise<void> {
  return new Promise<void>((resolve) => {
    varta.events.watch(id, resolve);
  });
}

function encode(descriptor: PermissionDescriptor): string {
  const head = DESCRIPTOR_VERSION + SEPARATOR + descriptor.name + SEPARATOR;
  const resource = descriptor.resource;
  if (resource === undefined) {
    return head + NO_RESOURCE + SEPARATOR;
  }
  return head + WITH_RESOURCE + SEPARATOR + resource;
}

// The Permissions API rejects an unknown permission name with a TypeError.
function invalid(descriptor: PermissionDescriptor): TypeError {
  return new TypeError(`'${descriptor.name}' is not a valid permission name for this application`);
}

function stateOf(code: number): PermissionState {
  if (code === GRANTED) {
    return 'granted';
  }
  if (code === PROMPT) {
    return 'prompt';
  }
  if (code === DENIED) {
    return 'denied';
  }
  throw new TypeError('invalid permission state');
}

function stateNamed(text: string): PermissionState {
  if (text === 'granted') {
    return 'granted';
  }
  if (text === 'prompt') {
    return 'prompt';
  }
  return 'denied';
}

export function permissionsQuery(descriptor: PermissionDescriptor): Promise<PermissionStatus> {
  if (descriptor.name.includes(SEPARATOR)) {
    return Promise.reject(invalid(descriptor));
  }
  const code = varta.permissions.state(encode(descriptor));
  if (code === INVALID_DESCRIPTOR) {
    return Promise.reject(invalid(descriptor));
  }
  const status: PermissionStatus = { name: descriptor.name, state: stateOf(code) };
  return Promise.resolve(status);
}

export async function permissionsRequest(
  descriptor: PermissionDescriptor,
): Promise<PermissionStatus> {
  if (descriptor.name.includes(SEPARATOR)) {
    throw invalid(descriptor);
  }
  const id = varta.permissions.ask(encode(descriptor));
  if (id === INVALID_DESCRIPTOR) {
    throw invalid(descriptor);
  }
  await waitFor(id);
  if (varta.events.take(id) !== KIND_EVENT) {
    throw new TypeError('invalid permission answer');
  }
  const status: PermissionStatus = { name: descriptor.name, state: stateNamed(varta.events.text(id)) };
  return status;
}
