// Engine-owned browser API bridge. Ticket policy/consumption is server-side.
const ticket=location.hash.slice(1);
history.replaceState(null,"",location.pathname);
try{
  const response=await fetch("/activate",{method:"POST",credentials:"same-origin",redirect:"error",cache:"no-store",referrerPolicy:"no-referrer",headers:{authorization:"Bearer "+ticket}});
  if(!response.ok)throw new Error("Preview activation unavailable");
  const result=await response.json();
  location.replace(result.url);
}catch{
  document.querySelector("#status").textContent="Preview activation unavailable. Request a new link in Studio.";
}
