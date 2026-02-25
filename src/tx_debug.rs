use bdk_wallet::bitcoin::Transaction;
use std::fmt::Write;

#[derive(Debug, Clone)]
struct MultiAInfo {
    threshold: usize,
    keys: Vec<[u8; 32]>,
}

#[derive(Debug, Clone)]
struct CsvPkInfo {
    csv_blocks: u32,
    key: [u8; 32],
}

#[derive(Debug, Clone)]
struct CltvPkInfo {
    lock_height: u32,
    key: [u8; 32],
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn short_hex(bytes: &[u8]) -> String {
    let full = bytes_to_hex(bytes);
    if full.len() <= 64 {
        return full;
    }
    format!("{}...{}", &full[..32], &full[full.len() - 32..])
}

fn short_key_id(bytes: &[u8]) -> String {
    let full = bytes_to_hex(bytes);
    if full.len() <= 8 {
        return full;
    }
    format!("{}...{}", &full[..4], &full[full.len() - 4..])
}

fn sats_to_btc_str(sats: u64) -> String {
    format!("{:.8}", sats as f64 / 100_000_000.0)
}

fn decode_small_int_opcode(op: u8) -> Option<usize> {
    if (0x51..=0x60).contains(&op) {
        Some((op - 0x50) as usize)
    } else {
        None
    }
}

fn parse_scriptnum(script: &[u8], idx: &mut usize) -> Option<u32> {
    if *idx >= script.len() {
        return None;
    }
    let op = script[*idx];
    if let Some(v) = decode_small_int_opcode(op) {
        *idx += 1;
        return Some(v as u32);
    }
    if op == 0x00 {
        *idx += 1;
        return Some(0);
    }
    // Direct pushes only (1..=5 bytes) are enough for locktime in these tests.
    if (0x01..=0x05).contains(&op) {
        let n = op as usize;
        let start = *idx + 1;
        let end = start + n;
        if end > script.len() {
            return None;
        }
        let mut value: u32 = 0;
        for (shift, b) in script[start..end].iter().enumerate() {
            value |= (*b as u32) << (8 * shift);
        }
        *idx = end;
        return Some(value);
    }
    None
}

fn decode_multia_tapscript(script: &[u8]) -> Option<MultiAInfo> {
    // Generic parser for multi_a(k, keys...)
    // Expected shape:
    //   <32B key1> OP_CHECKSIG
    //   (<32B keyN> OP_CHECKSIGADD)*
    //   OP_<k> OP_NUMEQUAL
    if script.len() < 36 {
        return None;
    }

    let mut i = 0usize;
    let mut keys = Vec::<[u8; 32]>::new();
    while i + 34 <= script.len() {
        if script[i] != 0x20 {
            return None;
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&script[i + 1..i + 33]);
        keys.push(key);
        i += 33;

        let opcode = script[i];
        if keys.len() == 1 {
            if opcode != 0xac {
                return None;
            }
        } else if opcode != 0xba {
            return None;
        }
        i += 1;

        if i + 2 == script.len() {
            break;
        }
    }

    if keys.is_empty() || i + 2 != script.len() {
        return None;
    }
    let k = decode_small_int_opcode(script[i])?;
    if script[i + 1] != 0x9c {
        return None;
    }
    if k == 0 || k > keys.len() {
        return None;
    }
    Some(MultiAInfo { threshold: k, keys })
}

fn decode_csv_pk_tapscript(script: &[u8]) -> Option<CsvPkInfo> {
    // Supported forms:
    // 1) OP_<n> OP_CSV OP_DROP <32B key> OP_CHECKSIG
    // 2) OP_<n> OP_CSV <32B key> OP_CHECKSIG
    // 3) OP_<n> OP_CSV OP_DROP <32B key> OP_CHECKSIGVERIFY
    // 4) OP_<n> OP_CSV <32B key> OP_CHECKSIGVERIFY
    // 5) OP_<n> OP_CSV OP_VERIFY <32B key> OP_CHECKSIG
    // 6) OP_<n> OP_CSV OP_VERIFY <32B key> OP_CHECKSIGVERIFY
    if script.len() < 36 {
        return None;
    }

    let mut i = 0usize;
    let n = parse_scriptnum(script, &mut i)?;
    if i >= script.len() || script[i] != 0xb2 {
        return None;
    }
    i += 1;
    if i < script.len() && (script[i] == 0x75 || script[i] == 0x69) {
        i += 1; // optional OP_DROP or OP_VERIFY
    }
    if i + 34 > script.len() || script[i] != 0x20 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&script[i + 1..i + 33]);
    let checksig_op = script[i + 33];
    if checksig_op != 0xac && checksig_op != 0xad {
        return None;
    }
    if i + 34 != script.len() {
        return None;
    }
    Some(CsvPkInfo {
        csv_blocks: n,
        key,
    })
}

fn decode_cltv_pk_tapscript(script: &[u8]) -> Option<CltvPkInfo> {
    // Supported forms:
    // OP_<t>|PUSHDATA(t) OP_CLTV [OP_DROP|OP_VERIFY] <32B key> OP_CHECKSIG[VERIFY]
    if script.len() < 36 {
        return None;
    }
    let mut i = 0usize;
    let t = parse_scriptnum(script, &mut i)?;
    if i >= script.len() || script[i] != 0xb1 {
        return None;
    }
    i += 1;
    if i < script.len() && (script[i] == 0x75 || script[i] == 0x69) {
        i += 1; // optional OP_DROP or OP_VERIFY
    }
    if i + 34 > script.len() || script[i] != 0x20 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&script[i + 1..i + 33]);
    let checksig_op = script[i + 33];
    if checksig_op != 0xac && checksig_op != 0xad {
        return None;
    }
    if i + 34 != script.len() {
        return None;
    }
    Some(CltvPkInfo {
        lock_height: t,
        key,
    })
}

fn plus_expression(keys: &[[u8; 32]]) -> String {
    let mut out = String::new();
    for (idx, key) in keys.iter().enumerate() {
        if idx > 0 {
            out.push_str(" + ");
        }
        out.push_str(&short_key_id(key));
    }
    out
}

pub fn format_tx_report(label: &str, tx: &Transaction) -> String {
    let mut out = String::new();
    let _ = writeln!(out);
    let _ = writeln!(out, "========== tx/{label} ==========");
    let _ = writeln!(
        out,
        "version={} lock_time={} txid={} inputs={} outputs={}",
        tx.version.0,
        tx.lock_time.to_consensus_u32(),
        tx.compute_txid(),
        tx.input.len(),
        tx.output.len()
    );

    for (i, input) in tx.input.iter().enumerate() {
        let _ = writeln!(
            out,
            "  input[{i}] prevout={}:{} seq={}",
            input.previous_output.txid,
            input.previous_output.vout,
            input.sequence.to_consensus_u32()
        );
        let _ = writeln!(out, "    witness_items={}", input.witness.len());
        if input.witness.is_empty() {
            let _ = writeln!(out, "      (none)");
        }

        let witness_len = input.witness.len();
        let (tapscript_idx, control_idx) = if witness_len >= 2 {
            (Some(witness_len - 2), Some(witness_len - 1))
        } else {
            (None, None)
        };
        let decoded_multia = tapscript_idx
            .and_then(|idx| {
                if idx < input.witness.len() {
                    Some(&input.witness[idx])
                } else {
                    None
                }
            })
            .and_then(|w| decode_multia_tapscript(w));
        let decoded_csv_pk = tapscript_idx
            .and_then(|idx| {
                if idx < input.witness.len() {
                    Some(&input.witness[idx])
                } else {
                    None
                }
            })
            .and_then(|w| decode_csv_pk_tapscript(w));
        let decoded_cltv_pk = tapscript_idx
            .and_then(|idx| {
                if idx < input.witness.len() {
                    Some(&input.witness[idx])
                } else {
                    None
                }
            })
            .and_then(|w| decode_cltv_pk_tapscript(w));
        let sig_items = witness_len.saturating_sub(2);

        for (j, wit) in input.witness.iter().enumerate() {
            let role = if Some(j) == control_idx {
                "control_block (taproot proof)"
            } else if Some(j) == tapscript_idx {
                "tapscript (spend condition)"
            } else {
                "stack_item"
            };

            let mut hint = String::new();
            if role == "stack_item" {
                if let Some(info) = &decoded_multia {
                    if j < sig_items {
                        // For multi_a witness stack, left-most stack items correspond
                        // to keys from right-to-left in the script.
                        let key_idx = info.keys.len().saturating_sub(1 + j);
                        if key_idx < info.keys.len() {
                            hint = format!(" signature for {}", short_key_id(&info.keys[key_idx]));
                        }
                    }
                } else if let Some(info) = &decoded_csv_pk {
                    if j < sig_items {
                        hint = format!(" signature for {}", short_key_id(&info.key));
                    }
                } else if let Some(info) = &decoded_cltv_pk {
                    if j < sig_items {
                        hint = format!(" signature for {}", short_key_id(&info.key));
                    }
                }
            }

            let _ = writeln!(
                out,
                "      witness[{i}][{j}] role={}{} len={} hex={}",
                role,
                hint,
                wit.len(),
                short_hex(wit)
            );

            if role == "tapscript (spend condition)" {
                if let Some(info) = &decoded_multia {
                    let _ = writeln!(
                        out,
                        "        decoded: multi_a({}-of-{}): {}",
                        info.threshold,
                        info.keys.len(),
                        plus_expression(&info.keys)
                    );
                } else if let Some(info) = &decoded_csv_pk {
                    let _ = writeln!(
                        out,
                        "        decoded: {} + dt={} (CSV)",
                        short_key_id(&info.key),
                        info.csv_blocks
                    );
                } else if let Some(info) = &decoded_cltv_pk {
                    let _ = writeln!(
                        out,
                        "        decoded: {} + t={} (CLTV)",
                        short_key_id(&info.key),
                        info.lock_height
                    );
                }
            }
        }
    }

    for (i, output) in tx.output.iter().enumerate() {
        let _ = writeln!(
            out,
            "  output[{i}] value_sats={} value_btc={} script={:?}",
            output.value.to_sat(),
            sats_to_btc_str(output.value.to_sat()),
            output.script_pubkey
        );
    }
    let _ = writeln!(out, "===============================");
    out
}

pub fn print_tx_report(label: &str, tx: &Transaction) {
    println!("{}", format_tx_report(label, tx));
}
