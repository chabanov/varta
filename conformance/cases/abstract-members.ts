abstract class Account {
  constructor(protected balance: number) {}
  abstract fee(): number;
  abstract get kind(): string;
  withdraw(amount: number): number {
    this.balance -= amount + this.fee();
    return this.balance;
  }
  describe(): string { return this.kind + ':' + String(this.balance); }
}
class Checking extends Account {
  fee(): number { return 1; }
  get kind(): string { return 'checking'; }
}
class Savings extends Account {
  fee(): number { return 0; }
  get kind(): string { return 'savings'; }
}
export function main(): void {
  const accounts: Account[] = [new Checking(100), new Savings(50)];
  for (const account of accounts) {
    account.withdraw(10);
    console.log(account.describe());
  }
}
