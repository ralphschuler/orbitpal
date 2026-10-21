/** Versioned declarative identity only: never activation, an offer lease or permission. */
export interface ContentRef {packageId:string;packageVersion:number;templateId:string}
export function parseContentRef(value:unknown):ContentRef {
  if(!value||typeof value!=='object'||Array.isArray(value))throw Error('pack_invalid_reference');
  const ref=value as Record<string,unknown>;
  if(typeof ref.packageId!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(ref.packageId)
    ||typeof ref.packageVersion!=='number'||!Number.isInteger(ref.packageVersion)||ref.packageVersion<1||ref.packageVersion>0xffff_ffff
    ||typeof ref.templateId!=='string'||!ref.templateId.startsWith(ref.packageId+':')||!/^[A-Za-z0-9_-]{1,96}$/.test(ref.templateId.slice(ref.packageId.length+1)))throw Error('pack_invalid_reference');
  return {packageId:ref.packageId,packageVersion:ref.packageVersion,templateId:ref.templateId};
}
export const sameContentRef=(a:ContentRef,b:ContentRef)=>a.packageId===b.packageId&&a.packageVersion===b.packageVersion&&a.templateId===b.templateId;
