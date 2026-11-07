class Table {
  static rows: string[] = [];
  static {
    Table.rows.push('header');
  }
}
export function main(): void {
  console.log(Table.rows.join(','));
}
