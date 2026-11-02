// Host-owned selectedness for the protocol's single-select / text-only options.
import {registry} from '../../../varta-common/browser/registry.mjs';
const nodeWork=registry.view.limits.selectNodeWork,encoder=new TextEncoder();
export const selectionTextBytes=text=>encoder.encode(text).byteLength;
const options = (nodes,id) => (nodes.get(id)?.children ?? []).filter(id=>nodes.get(id)?.tag==='option');
function ownerWork(nodes,id) {
  let work=0;
  for(let node=nodes.get(id);node;node=nodes.get(node.parent)) {
    work+=nodeWork;
    if(node.tag==='select')return [id,work];
    id=node.parent;
  }
  return [undefined,work];
}
export const selectOwner=(nodes,id)=>ownerWork(nodes,id)[0];
function optionWork(nodes,id){
  const node=nodes.get(id);if(node?.tag!=='option')return 0;
  let work=nodeWork+(node.optionValueBytes??0);
  for(const id of node.children){const child=nodes.get(id);if(child?.kind==='text')work+=nodeWork+child.textBytes;}
  return work;
}
const selectWork=(nodes,id)=>(nodes.get(id)?.children??[]).reduce((work,id)=>work+optionWork(nodes,id),0);
export function selectInventoryWork(nodes){let work=0;for(const id of nodes.keys())work+=optionWork(nodes,id);return work;}
export function selectOperationWork(nodes,op){
  let first,second,incoming=0;
  switch(op.name){
    case 'set_text':case 'remove':first=op.node;break;
    case 'insert':{
      first=op.node;second=op.parent;const child=nodes.get(op.node);
      incoming=child?.kind==='text'?nodeWork+child.textBytes:optionWork(nodes,op.node);break;
    }
    case 'set_attribute':case 'remove_attribute':{
      const node=nodes.get(op.node);
      if(node?.tag!=='option'||!['value','selected','disabled'].includes(op.attribute))return 0;
      const changed=op.name==='set_attribute'?node.attributes.get(op.attribute)!==op.text:node.attributes.has(op.attribute);
      if(!changed)return 0;first=op.node;break;
    }
    case 'set_value':{
      const node=nodes.get(op.node);
      if(node?.tag!=='select')return 0;
      first=op.node;break;
    }
    default:return 0;
  }
  const [a,aw]=ownerWork(nodes,first),[b,bw]=ownerWork(nodes,second);
  return aw+bw+incoming+(a===undefined?0:selectWork(nodes,a))+(b===undefined||b===a?0:selectWork(nodes,b))+selectionTextBytes(op.text??'');
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
  if(attribute==='value')node.optionValueBytes=present?selectionTextBytes(node.attributes.get('value')):0;
  const select=selectOwner(nodes,id);
  if(attribute==='selected'&&!node.optionDirty){
    node.optionSelected=present;
    if(select!==undefined)normalizeSelect(nodes,select,present?id:undefined);
  }else if(attribute==='value'&&select!==undefined)syncSelect(nodes,select);
}
export function syncSelectFor(nodes,id) {const owner=selectOwner(nodes,id);if(owner!==undefined)syncSelect(nodes,owner);}
