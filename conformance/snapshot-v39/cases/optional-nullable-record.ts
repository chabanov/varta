interface User { user_id: string }
interface Envelope { user?: User | null; total?: number | null }
function accepted(text: string, reverse: boolean): boolean {
 const raw: unknown = JSON.parse(text);
 const me = raw as Envelope;
 if (reverse) return me.user !== null && me.user !== undefined && me.user.user_id === 'fixture';
 return me.user !== undefined && me.user !== null && me.user.user_id === 'fixture';
}
function total(text: string): number {
 const raw: unknown = JSON.parse(text);
 const me = raw as Envelope;
 if (me.total !== null && me.total !== undefined) return me.total + 1;
 return 0;
}
export function main(): void {
 console.log(accepted('{}', false), accepted('{}', true));
 console.log(accepted('{"user":null}', false), accepted('{"user":null}', true));
 console.log(accepted('{"user":{"user_id":"fixture"}}', false), accepted('{"user":{"user_id":"fixture"}}', true));
 console.log(total('{}'), total('{"total":null}'), total('{"total":4}'));
}
