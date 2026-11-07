// ECMA-262 §10.4.2.4 ArraySetLength and §23.1.3 Array.prototype methods on
// arrays with an integrity level: each failing write or deletion throws a
// TypeError after the earlier steps took effect.
export function main(): void {
  const sealed = Object.seal([1, 2, 3]);
  sealed[0] = 5;
  try {
    sealed.shift();
  } catch (error) {
    console.log(String(error));
  }
  console.log(sealed.join());
  const frozen = Object.freeze([1, 2]);
  try {
    frozen.push(3);
  } catch (error) {
    console.log(String(error));
  }
  try {
    frozen.length = 0;
  } catch (error) {
    console.log(String(error));
  }
  console.log(`${Object.isFrozen(frozen)} ${Object.isSealed(sealed)} ${frozen.length}`);
}
