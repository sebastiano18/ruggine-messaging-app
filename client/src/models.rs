use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct RegisterReq<'a>{ pub username:&'a str, pub password:&'a str }
#[derive(Deserialize)]
pub struct LoginResp{ pub token:String }

#[derive(Serialize)]
pub struct GroupReq<'a>{ pub name:&'a str }

#[derive(Serialize)]
pub struct InviteReq{ pub group_id: i64 }
#[derive(Deserialize)]
pub struct InviteResp{ pub token:String }

#[derive(Serialize)]
pub struct JoinByTokenReq<'a>{ pub token: &'a str }

#[derive(Deserialize)]
pub struct MessageDto{ pub id:i64, pub author_id:i64, pub body:String, pub created_at:i64 }
#[derive(Serialize)]
pub struct SendMsgReq<'a>{ pub body:&'a str }
