// The docs/language-guide.md weak references example, verbatim.
class Session {
  readonly user: string;
  constructor(user: string) {
    this.user = user;
  }
}

const cache = new WeakMap<Session, string>();
const registry = new FinalizationRegistry<string>((user) => {
  console.log(`session of ${user} collected`);
});

export function main(): void {
  const session = new Session('ada');
  cache.set(session, 'profile');
  registry.register(session, session.user);
  const ref = new WeakRef(session);
  console.log(`${cache.get(session)} ${ref.deref()?.user}`);
}
