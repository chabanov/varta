import {registry} from '../../../varta-common/browser/registry.mjs';

const V=registry.view, CAP=V.limits.namingWork+1, NODE=V.limits.namingNodeWork;
const add=(a,b)=>Math.min(CAP,a+b);
const namingAttributes=['id','for','aria-label','aria-labelledby','alt','placeholder'];
const embedded=node=>node.tag==='select'||node.tag==='textarea'||node.tag==='input'&&!['checkbox','radio'].includes(node.attributes.get('type')??'text');
const roleOf=node=>node.attributes.get('role')??(node.tag==='input'?V.accessibility.inputRoles[node.attributes.get('type')??'text']:V.accessibility.implicitRoles[node.tag]??(node.tag==='section'&&(node.attributes.has('aria-label')||node.attributes.has('aria-labelledby'))?'region':undefined));

/** Final potential naming inventory. No visibility, edit or precedence discount. */
export function potentialNamingWork(nodes){
 const order=[],pending=[];
 for(const [id,node] of nodes)if(node.parent===V.wire.noNode)pending.push(id);
 while(pending.length){const id=pending.pop();order.push(id);for(const child of nodes.get(id).children)pending.push(child);}
 const own=new Map(),ids=new Map();let inventory=0;
 for(const id of order){
  const node=nodes.get(id);let cost=NODE;
  if(node.kind==='text')cost=add(cost,node.textBytes);
  else{
   for(const attr of namingAttributes)cost=add(cost,node.attributeBytes.get(attr)??0);
   if(node.attributes.has('id'))ids.set(node.attributes.get('id'),id);
  }
  own.set(id,cost);inventory=add(inventory,NODE);
 }
 const noRelations=new Map();
 for(const id of order.toReversed()){
  const node=nodes.get(id);let cost=own.get(id);
  if(node.kind==='element'&&embedded(node))cost=add(cost,V.limits.fieldValueReservationBytes);
  for(const child of node.children)cost=add(cost,noRelations.get(child));
  noRelations.set(id,cost);
 }
 const relations=new Map();
 for(const id of order){
  const node=nodes.get(id);let cost=0;
  for(const ident of (node.attributes?.get('aria-labelledby')??'').split(/[\t\n\f\r ]+/).filter(Boolean)){
   cost=add(cost,NODE+1);if(ids.has(ident))cost=add(cost,noRelations.get(ids.get(ident)));
  }
  relations.set(id,cost);
 }
 const descendants=new Map();
 for(const id of order.toReversed()){
  const node=nodes.get(id);let cost=add(own.get(id),relations.get(id));
  if(node.kind==='element'&&embedded(node))cost=add(cost,V.limits.fieldValueReservationBytes);
  for(const child of node.children)cost=add(cost,descendants.get(child));
  descendants.set(id,cost);
 }
 const labels=new Map();
 for(const id of order){
  const node=nodes.get(id);
  if(node.tag==='label'&&node.attributes.has('for')){
   const ident=node.attributes.get('for');labels.set(ident,add(labels.get(ident)??0,add(NODE+1,descendants.get(id))));
  }
 }
 for(const id of order){
  const node=nodes.get(id);if(id===V.wire.root||node.kind!=='element')continue;
  const role=roleOf(node);if(role===undefined)continue;
  let cost=add(own.get(id),relations.get(id));
  if(V.accessibility.nameFromContent.includes(role))for(const child of node.children)cost=add(cost,descendants.get(child));
  if(V.accessibility.labelable.includes(node.tag)&&node.attributes.has('id'))cost=add(cost,labels.get(node.attributes.get('id'))??0);
  inventory=add(inventory,cost);if(inventory===CAP)break;
 }
 return inventory;
}
