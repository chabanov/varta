import {registry} from '../../../varta-common/browser/registry.mjs';
import {compileVariableValue} from './view-variables.mjs';

const CAP=registry.view.limits.valueBytes/2;
const wide=new Set(['initial','inherit','unset','revert','revert-layer']);
export function compileExpansionValue(text){
 const raw=compileVariableValue(text);
 if(!raw)return {parts:[{units:CAP,wide:false}],references:new Set(),choice:true,plainUnits:Math.max(CAP,text.length)};
 const compile=value=>{
  const parts=value.parts.map(part=>typeof part==='string'?{units:part.length,wide:wide.has(part.trim().toLowerCase())}:{name:part.name,fallback:part.fallback&&compile(part.fallback)});
  return {parts,references:value.references,choice:parts.length<=1&&parts.every(part=>!part.fallback||part.fallback.choice)};
 };
 return {...compile(raw),plainUnits:text.length};
}
function partUpper(part,lookup){return part.name===undefined?part.units<=CAP?part.units:0:Math.max(lookup(part.name),part.fallback?upper(part.fallback,lookup):0);}
function upper(value,lookup){let units=0;for(const part of value.parts){const next=partUpper(part,lookup);if(next)units=Math.min(CAP,units+next+Number(units>0));}return units;}
function fresh(value,lookup){
 const positive=value.parts.filter(part=>partUpper(part,lookup)>0);if(!positive.length)return 0;if(positive.length>1)return upper(value,lookup);
 const part=positive[0];return part.name===undefined?part.wide?0:partUpper(part,lookup):part.fallback?fresh(part.fallback,lookup):0;
}
function terminal(value,lookup){let units=0;for(const part of value.parts)units=Math.max(units,part.name===undefined?partUpper(part,lookup):Math.max(lookup(part.name),part.fallback?terminal(part.fallback,lookup):0));return units;}

/** Candidate upper bounds, separate from the renderer's active-cycle scope. */
export function createExpansionScope(){
 const values=new Map();
 return {
  upper:value=>value.references.size?upper(value,name=>values.get(name)??0):value.plainUnits,
  extend(raw){
   const names=[...raw.keys()].sort(),indices=new Map(names.map((name,i)=>[name,i]));
   const graph=names.map(()=>[]),reverse=names.map(()=>[]);
   names.forEach((name,i)=>{
    const edges=new Set();for(const value of raw.get(name))for(const name of value.references)if(indices.has(name))edges.add(indices.get(name));
    for(const next of edges){graph[i].push(next);reverse[next].push(i);}
   });
   const seen=new Uint8Array(names.length),order=[];
   for(let root=0;root<names.length;root++){
    const work=[[root,false]];
    while(work.length){const [node,leave]=work.pop();if(leave){order.push(node);continue;}if(seen[node])continue;seen[node]=1;work.push([node,true]);for(const next of graph[node])work.push([next,false]);}
   }
   const component=new Int32Array(names.length).fill(-1),components=[];
   for(const root of order.reverse()){
    if(component[root]!==-1)continue;
    const id=components.length,members=[],work=[root];
    while(work.length){const node=work.pop();if(component[node]!==-1)continue;component[node]=id;members.push(node);for(const next of reverse[node])work.push(next);}
    components.push(members);
   }
   const bounds=names.map(()=>0);
   for(let id=components.length-1;id>=0;id--){
    const members=components[id],lookup=name=>indices.has(name)?component[indices.get(name)]===id?0:bounds[indices.get(name)]:values.get(name)??0;
    if(members.length>1||graph[members[0]].includes(members[0])){
     let seed=0,joins=false;for(const i of members){seed=Math.max(seed,values.get(names[i])??0);for(const value of raw.get(names[i])){seed=Math.max(seed,terminal(value,lookup));joins ||= !value.choice;}}
     const bound=seed&&joins?CAP:seed;for(const i of members)bounds[i]=bound;
    }else{const i=members[0];let bound=values.get(names[i])??0;for(const value of raw.get(names[i]))bound=Math.max(bound,upper(value,lookup));bounds[i]=bound;}
   }
   const lookup=name=>indices.has(name)?bounds[indices.get(name)]:values.get(name)??0;
   let units=0;for(const name of names){let bound=0;for(const value of raw.get(name))bound=Math.max(bound,fresh(value,lookup));units+=bound;}
   const previous=names.map(name=>[name,values.get(name)]);names.forEach((name,i)=>values.set(name,bounds[i]));return {previous,units};
  },
  restore(checkpoint){for(const [name,previous] of checkpoint.previous)if(previous===undefined)values.delete(name);else values.set(name,previous);},
 };
}

export function potentialExpandedValues(nodes,index,root,charge){
 const scope=createExpansionScope(),pending=[];
 for(const [id,node] of nodes)if(node.parent===registry.view.wire.noNode)pending.push({id});
 const add=(map,name,value)=>{if(!map.has(name))map.set(name,[]);map.get(name).push(value);};
 while(pending.length){
  const task=pending.pop();if(task.leave){scope.restore(task.leave);continue;}
  const node=nodes.get(task.id),custom=new Map(),properties=new Map();
  if(node.kind==='element'){
   const candidates=new Set(task.id===root?index.roots:[]);
   if(task.id!==root)for(const name of new Set(node.class.split(/[\t\n\f\r ]+/).filter(Boolean)))for(const rule of index.classes.get(name)??[])candidates.add(rule);
   for(const rule of candidates)for(const [name,value] of rule.expansion)add(name.startsWith('--')?custom:properties,name,value);
   for(const [name,text] of node.style)add(properties,name,compileExpansionValue(text));
  }
  const checkpoint=scope.extend(custom);charge(checkpoint.units*2);
  for(const candidates of properties.values()){let bound=0;for(const value of candidates)bound=Math.max(bound,scope.upper(value));charge(bound*2);}
  pending.push({leave:checkpoint});for(const id of node.children)pending.push({id});
 }
}
