// A generic method whose type argument is inferred from the contextual
// result type of the calling method (admitted by v37 `class_generics`).
interface User {
  id: number;
}
class Client {
  async request<T>(make: () => T): Promise<T> {
    return make();
  }
}
class Users {
  client: Client = new Client();
  get(id: number): Promise<User> {
    return this.client.request((): User => ({ id }));
  }
}
export async function main(): Promise<void> {
  const user = await new Users().get(7);
  console.log(String(user.id));
}
