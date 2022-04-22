use crate::Api;
use anyhow::Result;
use module::schema::DelegationInfo;
use poem_openapi::param::Query;
use poem_openapi::{payload::Json, ApiResponse, Object};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::chrono::Local;
use sqlx::Error::RowNotFound;
use sqlx::Row;
use std::collections::HashSet;

#[derive(ApiResponse)]
pub enum ChainStatisticsResponse {
    #[oai(status = 200)]
    Ok(Json<ChainStatisticsRes>),
}

#[derive(Serialize, Deserialize, Debug, Default, Object)]
pub struct ChainStatisticsRes {
    pub code: i32,
    pub message: String,
    pub data: Option<StatisticsData>,
}

#[derive(Serialize, Deserialize, Default, Debug, Object)]
pub struct StatisticsData {
    pub active_addresses: i64,
    pub total_txs: i64,
    pub daily_txs: i64,
}

#[derive(ApiResponse)]
pub enum StakingResponse {
    #[oai(status = 200)]
    Ok(Json<StakingRes>),
}

#[derive(Serialize, Deserialize, Debug, Default, Object)]
pub struct StakingRes {
    pub code: i32,
    pub message: String,
    pub data: Option<StakingData>,
}

#[derive(Serialize, Deserialize, Default, Debug, Object)]
pub struct StakingData {
    pub block_reward: u64,
    pub stake_ratio: f64,
    pub apy: f64,
    pub active_validators: Vec<String>,
}

pub async fn statistics(api: &Api) -> Result<ChainStatisticsResponse> {
    let mut conn = api.storage.lock().await.acquire().await?;

    let mut res_data = StatisticsData {
        active_addresses: 0,
        total_txs: 0,
        daily_txs: 0,
    };

    // total txs
    let sql_str = String::from("SELECT COUNT(*) as cnt FROM transaction");
    let total_txs_res = sqlx::query(sql_str.as_str()).fetch_one(&mut conn).await;
    if let Err(ref err) = total_txs_res {
        match err {
            RowNotFound => {}
            _ => {
                return Ok(ChainStatisticsResponse::Ok(Json(ChainStatisticsRes {
                    code: 50001,
                    message: "internal error, total txs.".to_string(),
                    data: Some(res_data),
                })));
            }
        }
    }
    let total_txs = total_txs_res.unwrap().try_get("cnt")?;

    // total address
    let sql_str = String::from("SELECT jsonb_path_query(value,'$.body.operations[*].TransferAsset.body.transfer.outputs[*].public_key') as addr FROM transaction");
    let active_addresses_res = sqlx::query(sql_str.as_str()).fetch_all(&mut conn).await;
    if let Err(ref err) = active_addresses_res {
        match err {
            RowNotFound => {}
            _ => {
                return Ok(ChainStatisticsResponse::Ok(Json(ChainStatisticsRes {
                    code: 50001,
                    message: "internal error, total addresses.".to_string(),
                    data: Some(res_data),
                })));
            }
        }
    }
    let vec = active_addresses_res.unwrap();
    let mut hs: HashSet<String> = HashSet::new();
    for row in vec {
        let value: Value = row.try_get("addr")?;
        let addr: String = serde_json::from_value(value).unwrap();
        hs.insert(addr);
    }
    let active_addresses = hs.len() as i64;

    // daily txs
    let t = Local::now().timestamp() - 3600 * 24;
    let daily_txs_res = sqlx::query("SELECT COUNT(*) as cnt FROM transaction where timestamp>=$1")
        .bind(t)
        .fetch_one(&mut conn)
        .await;
    if let Err(ref err) = daily_txs_res {
        match err {
            RowNotFound => {}
            _ => {
                return Ok(ChainStatisticsResponse::Ok(Json(ChainStatisticsRes {
                    code: 50001,
                    message: "internal error, daily txs.".to_string(),
                    data: Some(res_data),
                })));
            }
        }
    }
    let daily_txs = daily_txs_res.unwrap().try_get("cnt")?;

    res_data.daily_txs = daily_txs;
    res_data.total_txs = total_txs;
    res_data.active_addresses = active_addresses;

    Ok(ChainStatisticsResponse::Ok(Json(ChainStatisticsRes {
        code: 200,
        message: "".to_string(),
        data: Some(res_data),
    })))
}

pub async fn staking_info(api: &Api, height: Query<Option<i64>>) -> Result<StakingResponse> {
    let mut conn = api.storage.lock().await.acquire().await?;

    let sql_str = if let Some(height) = height.0 {
        format!("SELECT info FROM delegations WHERE height={}", height)
    } else {
        "SELECT info FROM delegations ORDER BY height DESC LIMIT 1".to_string()
    };
    let delegation_res = sqlx::query(sql_str.as_str()).fetch_one(&mut conn).await;

    if let Err(ref err) = delegation_res {
        return match err {
            RowNotFound => Ok(StakingResponse::Ok(Json(StakingRes {
                code: 200,
                message: "".to_string(),
                data: Some(StakingData::default()),
            }))),
            _ => Ok(StakingResponse::Ok(Json(StakingRes {
                code: 50001,
                message: "internal error.".to_string(),
                data: None,
            }))),
        };
    }
    let info_value: Value = delegation_res.unwrap().try_get("info")?;
    let delegation_info: DelegationInfo = serde_json::from_value(info_value).unwrap();

    let mut active_validators: Vec<String> = vec![];
    for (id, _) in delegation_info.validator_addr_map {
        active_validators.push(id);
    }
    let mut reward: u64 = 0;
    let mut total_stake: u64 = 0;
    for (_, dl) in delegation_info.global_delegation_records_map {
        reward += dl.rwd_amount;
        for (_, amount) in dl.delegations {
            total_stake += amount
        }
    }

    let data = StakingData {
        block_reward: reward,
        apy: delegation_info.return_rate.value,
        stake_ratio: total_stake as f64 / 21_420_000_000_000_000.0,
        active_validators,
    };

    Ok(StakingResponse::Ok(Json(StakingRes {
        code: 200,
        message: "".to_string(),
        data: Some(data),
    })))
}
