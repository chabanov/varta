// Host-owned selectedness for the protocol's single-select / text-only options.
const options = (nodes,id) => (nodes.get(id)?.children ?? []).filter(id=>nodes.get(id)?.tag==='option');
export function selectOwner(nodes,id) {
  for(let node=nodes.get(id);node;node=nodes.get(node.parent)) {
    if(node.tag==='select')return id;
    id=node.parent;
  }
}
export function optionText(nodes,id) {
  const stack=[id],parts=[];
  while(stack.length){const node=nodes.get(stack.pop());if(!node)continue;
    if(node.kind==='text')parts.push(node.text);else stack.push(...node.children.toReversed());}
  return parts.join('').split(/[\t\n\f\r ]+/).filter(Boolean).join(' ');
}
export const optionValue=(nodes,id)=>nodes.get(id).attributes.get('value')??optionText(nodes,id);
export function syncSelect(nodes,id) {
  const node=nodes.get(id);if(node?.tag!=='select')return;
  const selected=options(nodes,id).find(id=>nodes.get(id).optionSelected);
  node.value=selected===undefined?'':optionValue(nodes,selected);
}
function setSelectOption(nodes,id,selected) {
  for(const option of options(nodes,id)){
    const node=nodes.get(option);node.optionSelected=option===selected;
    if(node.optionSelected)node.optionDirty=true;
  }
  syncSelect(nodes,id);
}
export function setSelectValue(nodes,id,value) {
  setSelectOption(nodes,id,options(nodes,id).find(option=>optionValue(nodes,option)===value));
}
export function syncSelectChoice(nodes,id,value,selected) {
  // Parent records can outlive an option mutation. Use the exact choice while
  // it still belongs to this select and denotes the recorded value.
  if(selected===0&&value==='')setSelectOption(nodes,id,undefined);
  else if(selected!==undefined&&options(nodes,id).includes(selected)&&optionValue(nodes,selected)===value)setSelectOption(nodes,id,selected);
  else if(nodes.get(id).value!==value)setSelectValue(nodes,id,value);
}
export function normalizeSelect(nodes,id,preferred) {
  const list=options(nodes,id);
  const selected=list.includes(preferred)?preferred:list.findLast(id=>nodes.get(id).optionSelected)
    ??list.find(id=>!nodes.get(id).attributes.has('disabled'));
  for(const option of list)nodes.get(option).optionSelected=option===selected;
  syncSelect(nodes,id);
}
export function optionAttribute(nodes,id,attribute,present) {
  const node=nodes.get(id);if(node.tag!=='option')return;
  const select=selectOwner(nodes,id);
  if(attribute==='selected'&&!node.optionDirty){
    node.optionSelected=present;
    if(select!==undefined)normalizeSelect(nodes,select,present?id:undefined);
  }else if(attribute==='value'&&select!==undefined)syncSelect(nodes,select);
}
export function syncSelectFor(nodes,id) {const owner=selectOwner(nodes,id);if(owner!==undefined)syncSelect(nodes,owner);}
