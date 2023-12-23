use {
  super::*,
  axum::{
    response::Json,
    extract::{Extension, Path},
    extract,
  },
  serde_json::{Value, json},
  base64::{Engine as _, engine::general_purpose},
};


pub(super) struct Rest {

}


impl Rest {

  pub async fn inscription(
    Extension(server_config): Extension<Arc<ServerConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(inscription_id): Path<InscriptionId>,
  ) -> ServerResult<Json<Value>>{

    Ok(
      Json(_get_inscription(&server_config, &index, inscription_id)?)
    )
  }

  pub async fn inscriptions(
    Extension(server_config): Extension<Arc<ServerConfig>>,
    Extension(index): Extension<Arc<Index>>,
    extract::Json(inscription_ids): extract::Json<Vec<InscriptionId>>,
  ) -> ServerResult<Json<Vec<Value>>>{
    let mut _inscriptions: Vec<Value> = vec![];

    for inscription_id in inscription_ids {
      let inscription = _get_inscription(&server_config, &index, inscription_id)?;
      _inscriptions.push(inscription);
    }

    Ok(
      Json(_inscriptions)
    )
  }

  pub async fn sat(
    Extension(server_config): Extension<Arc<ServerConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(DeserializeFromStr(sat)): Path<DeserializeFromStr<Sat>>,
  ) -> ServerResult<Json<Value>> {
    let satpoint = index.rare_sat_satpoint(sat)?;
    Ok(
      Json(json!({
        "sat": sat.n().to_string(),
        "decimal": sat.decimal().to_string(),
        "degree": sat.degree().to_string(),
        "percentile": sat.percentile(),
        "name": sat.name(),
        "cycle": sat.cycle(),
        "epoch": sat.epoch().to_string(),
        "period": sat.period(),
        "block": sat.height().to_string(),
        "offset": sat.third(),
        "rarity": sat.rarity(),
        "satpoint": satpoint.unwrap_or(SatPoint {
          outpoint: OutPoint::null(),
            offset: 0,
          }),
        "blocktime": index.block_time(sat.height())?.timestamp().timestamp(),
        // "inscription": index.get_inscription_id_by_sat(sat)?
      }))
    )
  }

  pub async fn output(
    Extension(server_config): Extension<Arc<ServerConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(outpoint): Path<OutPoint>,
  ) -> ServerResult<Json<Value>> {
    let (output, inscriptions, address, ranges) = _output(&index, outpoint, &server_config)?;

    Ok(
      Json(json!({
        "outpoint": outpoint,
        "inscriptions": inscriptions,
        "script_pubkey": output.script_pubkey.to_asm_string(),
        "address": address,
        "output": output,
        "ranges": ranges
      }))
    )
  }

  pub async fn outputs(
    Extension(server_config): Extension<Arc<ServerConfig>>,
    Extension(index): Extension<Arc<Index>>,
    extract::Json(outpoints): extract::Json<Vec<OutPoint>>
  ) -> ServerResult<Json<Vec<Value>>> {

    let mut _outputs: Vec<Value> = vec![];

    for outpoint in outpoints {
      let (output, inscriptions, address, ranges) = _output(&index, outpoint, &server_config)?;

      _outputs.push(json!({
        "outpoint": outpoint,
        "inscriptions": inscriptions,
        "script_pubkey": output.script_pubkey.to_asm_string(),
        "address": address,
        "output": output,
        "ranges": ranges
      }));
    }

    Ok(
      Json(_outputs)
    )

  }

}

fn _output(index: &Arc<Index>, outpoint: OutPoint, server_config: &Arc<ServerConfig>) -> Result<(TxOut, Vec<InscriptionId>, String, Vec<Value>), ServerError> {
    let list = if index.has_sat_index() {
      index.list(outpoint)?
    } else {
      None
    };
    let output = if outpoint == OutPoint::null() || outpoint == unbound_outpoint() {
      let mut value = 0;

      if let Some(List::Unspent(ranges)) = &list {
        for (start, end) in ranges {
          value += end - start;
        }
      }

      TxOut {
        value,
        script_pubkey: ScriptBuf::new(),
      }
    } else {
      index
        .get_transaction(outpoint.txid)?
        .ok_or_not_found(|| format!("output {outpoint}"))?
        .output
        .into_iter()
        .nth(outpoint.vout as usize)
        .ok_or_not_found(|| format!("output {outpoint}"))?
    };
    let inscriptions = index.get_inscriptions_on_output(outpoint)?;
    let mut address: String = "".to_owned();
    if let Ok(_address) = server_config.chain.address_from_script(&output.script_pubkey) {
      address = _address.to_string();
    }
    let ranges = match list {
      Some(range) => match range {
        List::Unspent(ranges) => ranges.iter().map(|(start, end)| json!({"start": start, "end": end})).collect(),
        List::Spent => vec![]
      },
      None => vec![]
    };
    Ok((output, inscriptions, address, ranges))
}


fn _get_inscription(
  server_config: &Arc<ServerConfig>,
  index: &Arc<Index>,
  inscription_id: InscriptionId,
) -> Result<Value, ServerError>{

  let entry = index
    .get_inscription_entry(inscription_id)?
    .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

  let inscription = index
    .get_inscription_by_id(inscription_id)?
    .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

  let satpoint = index
    .get_inscription_satpoint_by_id(inscription_id)?
    .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

  let output = if satpoint.outpoint == unbound_outpoint() {
    None
  } else {
    Some(
      index
        .get_transaction(satpoint.outpoint.txid)?
        .ok_or_not_found(|| format!("inscription {inscription_id} current transaction"))?
        .output
        .into_iter()
        .nth(satpoint.outpoint.vout.try_into().unwrap())
        .ok_or_not_found(|| format!("inscription {inscription_id} current transaction output"))?,
    )
  };

  let mut address: String = "".to_owned();
  if let Some(_output) = output.clone() {
    address = match server_config.chain.address_from_script(&_output.script_pubkey) {
      Ok(_address) => _address.to_string(),
      Err(_) => "".to_owned()
    }
  }

  let mut content: String = "".to_owned();
  if let Some(_body) = inscription.clone().into_body() {
    content = general_purpose::STANDARD.encode(&_body);
  }

  Ok(
    json!({
      "genesis_fee": entry.fee,
      "genesis_height": entry.height,
      "content_length": inscription.content_length().unwrap_or(0),
      "content_type": inscription.content_type().unwrap_or(""),
      "content": content,
      "inscription_id": inscription_id,
      "address": address,
      "number": entry.inscription_number,
      "output": match output {
        Some(v) => v,
        None => TxOut::default()
      },
      "sat": match entry.sat {
          Some(sat) => sat.n().to_string(),
          None => "-1".to_owned(),
      },
      "satpoint": satpoint,
      "timestamp": entry.timestamp,
     })
  )
}
