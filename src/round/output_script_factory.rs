use bdk_wallet::bitcoin::{
    ScriptBuf, XOnlyPublicKey,
    opcodes,
    script::Builder,
    secp256k1::Secp256k1,
    taproot::{TaprootBuilder, TaprootBuilderError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputScriptSpec {
    pub script_pubkey: ScriptBuf,
    pub cooperative_tapscript: ScriptBuf,
    pub timeout_tapscript: ScriptBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputScriptFactory {
    recycle_block: u32,
    leaf_timeout: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputScriptFactoryError {
    EmptyUserKeys,
    TaprootBuild(TaprootBuilderError),
    TaprootFinalize,
}

impl From<TaprootBuilderError> for OutputScriptFactoryError {
    fn from(value: TaprootBuilderError) -> Self {
        Self::TaprootBuild(value)
    }
}

impl From<TaprootBuilder> for OutputScriptFactoryError {
    fn from(_value: TaprootBuilder) -> Self {
        Self::TaprootFinalize
    }
}

impl OutputScriptFactory {
    pub fn new(recycle_block: u32, leaf_timeout: u32) -> Self {
        Self {
            recycle_block,
            leaf_timeout,
        }
    }

    pub fn create_branch_output(
        &self,
        user_keys: &[XOnlyPublicKey],
        asp_key: XOnlyPublicKey,
    ) -> Result<OutputScriptSpec, OutputScriptFactoryError> {
        if user_keys.is_empty() {
            return Err(OutputScriptFactoryError::EmptyUserKeys);
        }

        let mut all_keys = Vec::with_capacity(user_keys.len() + 1);
        all_keys.extend_from_slice(user_keys);
        all_keys.push(asp_key);

        let cooperative_tapscript = build_multi_a_script(&all_keys);
        let timeout_tapscript = build_cltv_pk_script(self.recycle_block, asp_key);
        let script_pubkey =
            build_taproot_script_pubkey(asp_key, &cooperative_tapscript, &timeout_tapscript)?;

        Ok(OutputScriptSpec {
            script_pubkey,
            cooperative_tapscript,
            timeout_tapscript,
        })
    }

    pub fn create_leaf_output(
        &self,
        user_key: XOnlyPublicKey,
        asp_key: XOnlyPublicKey,
    ) -> Result<OutputScriptSpec, OutputScriptFactoryError> {
        let cooperative_tapscript = build_multi_a_script(&[user_key, asp_key]);
        let timeout_tapscript = build_csv_pk_script(self.leaf_timeout, user_key);
        let script_pubkey =
            build_taproot_script_pubkey(asp_key, &cooperative_tapscript, &timeout_tapscript)?;

        Ok(OutputScriptSpec {
            script_pubkey,
            cooperative_tapscript,
            timeout_tapscript,
        })
    }
}

fn build_multi_a_script(keys: &[XOnlyPublicKey]) -> ScriptBuf {
    let mut builder = Builder::new()
        .push_x_only_key(&keys[0])
        .push_opcode(opcodes::all::OP_CHECKSIG);

    for key in &keys[1..] {
        builder = builder
            .push_x_only_key(key)
            .push_opcode(opcodes::all::OP_CHECKSIGADD);
    }

    builder
        .push_int(keys.len() as i64)
        .push_opcode(opcodes::all::OP_NUMEQUAL)
        .into_script()
}

fn build_cltv_pk_script(lock_height: u32, key: XOnlyPublicKey) -> ScriptBuf {
    Builder::new()
        .push_int(lock_height as i64)
        .push_opcode(opcodes::all::OP_CLTV)
        .push_opcode(opcodes::all::OP_VERIFY)
        .push_x_only_key(&key)
        .push_opcode(opcodes::all::OP_CHECKSIG)
        .into_script()
}

fn build_csv_pk_script(csv_blocks: u32, key: XOnlyPublicKey) -> ScriptBuf {
    Builder::new()
        .push_int(csv_blocks as i64)
        .push_opcode(opcodes::all::OP_CSV)
        .push_opcode(opcodes::all::OP_VERIFY)
        .push_x_only_key(&key)
        .push_opcode(opcodes::all::OP_CHECKSIG)
        .into_script()
}

fn build_taproot_script_pubkey(
    internal_key: XOnlyPublicKey,
    cooperative_tapscript: &ScriptBuf,
    timeout_tapscript: &ScriptBuf,
) -> Result<ScriptBuf, OutputScriptFactoryError> {
    let secp = Secp256k1::verification_only();
    let builder = TaprootBuilder::new()
        .add_leaf(1, cooperative_tapscript.clone())?
        .add_leaf(1, timeout_tapscript.clone())?;
    let spend_info = builder.finalize(&secp, internal_key)?;
    Ok(ScriptBuf::new_p2tr_tweaked(spend_info.output_key()))
}
