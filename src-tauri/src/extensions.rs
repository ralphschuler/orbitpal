use serde_json::{json,Value};
use std::collections::BTreeSet;

pub fn legacy_definition(server:&Value)->Result<Value,String>{
    if !server.is_object(){return Err("Vorhandene MCP-Definition muss ein Objekt sein".into());}
    let mut value=server.clone();
    if let Some(fields)=value.as_object_mut(){fields.remove("enabled");fields.remove("allowedTools");fields.remove("viewerAllowedTools");}
    if !value["id"].as_str().is_some_and(crate::storage::valid_id){value["id"]=json!(uuid::Uuid::new_v4().to_string());}
    if matches!(value["transport"].as_str(),Some("streamable-http"|"streamableHttp")){value["transport"]=json!("http");}
    // Older profiles may contain unfinished, disabled drafts. Preserve those
    // for editing instead of making application startup fail validation.
    Ok(crate::storage::sanitize(value))
}

pub fn definition(server:&Value)->Result<Value,String>{
    if !server.is_object(){return Err("MCP-Definition muss ein Objekt sein".into());}
    let id=server["id"].as_str().filter(|id|!id.is_empty()).map(str::to_owned).unwrap_or_else(||uuid::Uuid::new_v4().to_string());
    if !crate::storage::valid_id(&id){return Err("Ungültige MCP-Server-ID".into());}
    let mut value=json!({"id":id,"name":server["name"].as_str().unwrap_or("MCP-Server"),"transport":match server["transport"].as_str().unwrap_or("stdio"){"http"|"streamable-http"|"streamableHttp"=>"http",other=>other}});
    if value["name"].as_str().is_some_and(|s|s.len()>160||s.chars().any(char::is_control)){return Err("MCP-Name ist ungültig".into());}
    if value["transport"]=="builtin"{
        if server["builtin"]!="cohost-control"{return Err("Unbekannter integrierter MCP-Server".into());}
        if ["command","args","env","cwd","url","apiKeyId","secretId","tokenEnv","headers"].iter().any(|key|server.get(*key).is_some_and(|value|!value.is_null())){return Err("Der integrierte Control-MCP speichert ausschließlich seinen Marker; URL und Zugriffsschlüssel gelten nur zur Laufzeit".into());}
        value["builtin"]=json!("cohost-control");return Ok(value);
    }
    for key in ["command","args","env","cwd","url","apiKeyId","secretId","tokenEnv"]{
        if let Some(field)=server.get(key).filter(|field|!field.is_null()){value[key]=field.clone();}
    }
    match value["transport"].as_str().unwrap_or_default(){
        "stdio"=>{
            if !value["command"].as_str().is_some_and(|s|!s.trim().is_empty()&&s.len()<=4096&&!s.contains('\0')){return Err("MCP-Startbefehl fehlt oder ist ungültig".into());}
            if let Some(args)=value.get("args"){
                if !args.as_array().is_some_and(|a|a.len()<=256&&a.iter().all(|v|v.as_str().is_some_and(|s|s.len()<=16_384&&!s.contains('\0')))){return Err("MCP-Argumente müssen eine begrenzte Liste von Texten sein".into());}
            }
        },
        "http"=>{
            let url=reqwest::Url::parse(value["url"].as_str().unwrap_or_default()).map_err(|_|"MCP-URL ist ungültig")?;
            if !matches!(url.scheme(),"http"|"https")||!url.username().is_empty()||url.password().is_some(){return Err("MCP benötigt HTTP(S) ohne Zugangsdaten in der URL".into());}
        },
        _=>return Err("MCP-Transport muss stdio, http oder ein freigegebener builtin sein".into()),
    }
    if let Some(env)=value.get("env"){
        if !env.as_object().is_some_and(|fields|fields.len()<=128&&fields.iter().all(|(key,value)|!key.is_empty()&&key.len()<=256&&!key.contains(['=','\0'])&&value.as_str().is_some_and(|s|s.len()<=32_768&&!s.contains('\0')))){return Err("MCP-Umgebung muss gültige Textwerte enthalten".into());}
    }
    for key in ["cwd","apiKeyId","secretId","tokenEnv"]{if value.get(key).is_some_and(|v|!v.as_str().is_some_and(|s|s.len()<=4096&&!s.contains('\0'))){return Err(format!("MCP-Feld {key} ist ungültig"));}}
    Ok(crate::storage::sanitize(value))
}

// Name and grants do not identify a transport. Credential references do: two
// installations using different accounts must not be collapsed into one.
pub fn signature(server:&Value)->Value{
    let mut value=server.clone();if let Some(fields)=value.as_object_mut(){for key in ["id","name","enabled","allowedTools","viewerAllowedTools"]{fields.remove(key);}}value
}
pub fn binding(server:&Value)->Result<Value,String>{
    let id=server["id"].as_str().filter(|id|crate::storage::valid_id(id)).ok_or("MCP-Bindung benötigt eine gültige ID")?;
    let mut result=json!({"id":id,"enabled":server["enabled"].as_bool().unwrap_or(false),"allowedTools":[],"viewerAllowedTools":[]});
    for key in ["allowedTools","viewerAllowedTools"]{
        if let Some(values)=server.get(key){
            let values=values.as_array().ok_or("MCP-Werkzeugfreigaben müssen Listen sein")?;
            if values.len()>1000||values.iter().any(|v|!v.as_str().is_some_and(|s|!s.is_empty()&&s.len()<=512&&!s.chars().any(char::is_control))){return Err("Ungültige MCP-Werkzeugfreigaben".into());}
            let unique:BTreeSet<&str>=values.iter().filter_map(Value::as_str).collect();result[key]=json!(unique);
        }
    }
    Ok(result)
}
fn grants(value:&Value,key:&str)->BTreeSet<String>{value[key].as_array().into_iter().flatten().filter_map(|v|v.as_str().map(str::to_owned)).collect()}
fn union(left:&mut BTreeSet<String>,right:BTreeSet<String>){left.extend(right);if left.contains("*"){left.retain(|value|value=="*");}}
pub fn merge_bindings(bindings:Vec<Value>)->Vec<Value>{
    let mut result:Vec<Value>=Vec::new();
    for next in bindings{
        if let Some(current)=result.iter_mut().find(|value|value["id"]==next["id"]){
            if next["enabled"]!=true{continue;}
            if current["enabled"]!=true{*current=next;continue;}
            let effective=|binding:&Value|{let allowed=grants(binding,"allowedTools");let viewer=grants(binding,"viewerAllowedTools");if allowed.contains("*"){viewer}else if viewer.contains("*"){allowed}else{allowed.intersection(&viewer).cloned().collect()}};
            let mut viewers=effective(current);union(&mut viewers,effective(&next));
            let mut allowed=grants(current,"allowedTools");union(&mut allowed,grants(&next,"allowedTools"));
            current["allowedTools"]=json!(allowed);current["viewerAllowedTools"]=json!(viewers);
        }else{result.push(next);}
    }
    result
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn deduplication_never_combines_inactive_or_disjoint_viewer_grants(){
        let bindings=merge_bindings(vec![binding(&json!({"id":"one","enabled":true,"allowedTools":["a"],"viewerAllowedTools":["b"]})).unwrap(),binding(&json!({"id":"one","enabled":true,"allowedTools":["b"],"viewerAllowedTools":[]})).unwrap(),binding(&json!({"id":"one","enabled":false,"allowedTools":["*"],"viewerAllowedTools":["*"]})).unwrap()]);
        assert!(crate::mcp::permitted(&bindings[0],"a",false));assert!(crate::mcp::permitted(&bindings[0],"b",false));assert!(!crate::mcp::permitted(&bindings[0],"a",true));assert!(!crate::mcp::permitted(&bindings[0],"b",true));assert!(!crate::mcp::permitted(&bindings[0],"c",false));
    }
    #[test]
    fn same_transport_ignores_name_and_grants_but_preserves_account_identity(){
        let a=definition(&json!({"id":"a","name":"One","transport":"stdio","command":"node","args":["server.js"],"apiKeyId":"account-a","enabled":true,"allowedTools":["*"]})).unwrap();
        let b=definition(&json!({"id":"b","name":"Two","transport":"stdio","command":"node","args":["server.js"],"apiKeyId":"account-a"})).unwrap();
        assert_eq!(signature(&a),signature(&b));assert!(a.get("enabled").is_none());let mut c=b;c["apiKeyId"]=json!("account-b");assert_ne!(signature(&a),signature(&c));
    }
    #[test]
    fn builtin_control_definition_is_a_portable_marker_without_endpoint_or_credentials(){
        let marker=json!({"id":"cohost-control","transport":"builtin","builtin":"cohost-control","name":"Cohost"});
        assert_eq!(definition(&marker).unwrap(),marker);
        let mut unknown=marker.clone();unknown["builtin"]=json!("arbitrary-command");assert!(definition(&unknown).is_err());
        for key in ["url","command","apiKeyId","headers"]{let mut leaked=marker.clone();leaked[key]=json!("private runtime value");assert!(definition(&leaked).is_err());}
    }
}
