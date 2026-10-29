type AuthType = "none" | "apiKey";

const DEFAULT_BASE_URL = "https://api.example";
const DEFAULT_AUTH: AuthType = "apiKey";

class Client {
  readonly baseUrl: string = DEFAULT_BASE_URL;
  auth: AuthType = DEFAULT_AUTH;
  retries: 0 | 1 | 2 = 2;
  url(path: string): string {
    return `${this.baseUrl}${path}?auth=${this.auth}&retries=${this.retries}`;
  }
  useAuth(text: string): boolean {
    if (text === "none" || text === "apiKey") {
      this.auth = text;
      return true;
    }
    return false;
  }
}

export function main(): void {
  const client = new Client();
  console.log(client.url("/agents"));
  console.log(client.useAuth("none"));
  console.log(client.useAuth("bearer"));
  client.retries = 0;
  console.log(client.url("/runs"));
  console.log(JSON.stringify(client));
}
