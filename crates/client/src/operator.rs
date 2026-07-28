use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use cdda_net::{read_control_frame, write_control_frame};
use cdda_protocol::{
    ADMIN_ALPN, AccountId, AccountKeyRequest, AccountKeyResponse, AccountRole, AccountStatus,
    ActorId, AdminHello, AdminRequest, AdminResponse, ContentIdentity, ControlMessage,
    EndpointBindingSummary, EndpointIdentity, GAME_ALPN, ItemId, MAX_ADMIN_ACCOUNTS_PER_PAGE,
    MAX_ADMIN_INVENTORY_PER_PAGE, MAX_MODERATION_DURATION_SECONDS, MAX_MODERATION_HISTORY_PER_PAGE,
    MAX_REPORTS_PER_PAGE, PROTOCOL_VERSION, ReportId, ReportState,
};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey, endpoint::presets};

use crate::{pin_server, require_datagram_support, verify_existing_pin};

const OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) enum OneShotOperation {
    AccountKey(AccountKeyRequest),
    Admin(AdminRequest),
}

pub(crate) fn parse_account_key_operation(
    command: &str,
    endpoint: Option<&str>,
) -> Result<AccountKeyRequest, String> {
    match (command, endpoint) {
        ("list", None) => Ok(AccountKeyRequest::List),
        ("add", Some(endpoint)) => Ok(AccountKeyRequest::Add {
            endpoint: parse_endpoint(endpoint)?,
        }),
        ("revoke", Some(endpoint)) => Ok(AccountKeyRequest::Revoke {
            endpoint: parse_endpoint(endpoint)?,
        }),
        ("list", Some(_)) => Err(String::from("account-key list takes no endpoint ID")),
        ("add" | "revoke", None) => Err(format!("account-key {command} requires an endpoint ID")),
        _ => Err(format!("unknown account-key command: {command}")),
    }
}

pub(crate) fn parse_admin_operation(arguments: &[String]) -> Result<AdminRequest, String> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(admin_usage());
    };
    let values = &arguments[1..];
    match command {
        "list-accounts" => {
            require_range(command, values, 0, 2)?;
            Ok(AdminRequest::ListAccounts {
                after: values
                    .first()
                    .map(|value| parse_account_id(value))
                    .transpose()?,
                limit: parse_limit(values.get(1), MAX_ADMIN_ACCOUNTS_PER_PAGE, "account page")?,
            })
        }
        "list-characters" => {
            require_exact(command, values, 1)?;
            Ok(AdminRequest::ListCharacters {
                account_id: parse_account_id(&values[0])?,
            })
        }
        "inspect-character" => {
            require_range(command, values, 1, 3)?;
            let actor_id = parse_actor_id(&values[0])?;
            Ok(AdminRequest::InspectCharacter {
                actor_id,
                inventory_after: values
                    .get(1)
                    .map(|value| parse_item_id(value))
                    .transpose()?,
                inventory_limit: parse_limit(
                    values.get(2),
                    MAX_ADMIN_INVENTORY_PER_PAGE,
                    "inventory page",
                )?,
            })
        }
        "list-reports" => {
            require_range(command, values, 0, 3)?;
            let state = values
                .first()
                .map(|value| parse_report_filter(value))
                .transpose()?
                .flatten();
            Ok(AdminRequest::ListReports {
                state,
                after: values
                    .get(1)
                    .map(|value| parse_report_id(value))
                    .transpose()?,
                limit: parse_limit(values.get(2), MAX_REPORTS_PER_PAGE, "report page")?,
            })
        }
        "history" => {
            require_range(command, values, 1, 3)?;
            Ok(AdminRequest::ListModerationHistory {
                account_id: parse_account_id(&values[0])?,
                after: values
                    .get(1)
                    .map(|value| parse_u64(value, "history ID"))
                    .transpose()?,
                limit: parse_limit(
                    values.get(2),
                    MAX_MODERATION_HISTORY_PER_PAGE,
                    "history page",
                )?,
            })
        }
        "role" => {
            require_exact(command, values, 2)?;
            Ok(AdminRequest::SetRole {
                account_id: parse_account_id(&values[0])?,
                role: parse_role(&values[1])?,
            })
        }
        "status" => {
            require_exact(command, values, 2)?;
            Ok(AdminRequest::SetStatus {
                account_id: parse_account_id(&values[0])?,
                status: parse_status(&values[1])?,
            })
        }
        "suspend" | "mute" => {
            require_exact(command, values, 2)?;
            let account_id = parse_account_id(&values[0])?;
            let duration_seconds = parse_duration(&values[1])?;
            if command == "suspend" {
                Ok(AdminRequest::SetSuspension {
                    account_id,
                    duration_seconds,
                })
            } else {
                Ok(AdminRequest::SetMute {
                    account_id,
                    duration_seconds,
                })
            }
        }
        "kick" => {
            require_exact(command, values, 1)?;
            Ok(AdminRequest::Kick {
                account_id: parse_account_id(&values[0])?,
            })
        }
        "transfer" => {
            require_exact(command, values, 2)?;
            Ok(AdminRequest::TransferCharacter {
                actor_id: parse_actor_id(&values[0])?,
                new_owner: parse_account_id(&values[1])?,
            })
        }
        "resolve-report" => {
            require_exact(command, values, 2)?;
            let state = match values[1].as_str() {
                "actioned" => ReportState::Actioned,
                "dismissed" => ReportState::Dismissed,
                other => {
                    return Err(format!(
                        "report resolution must be actioned or dismissed, not {other}"
                    ));
                }
            };
            Ok(AdminRequest::SetReportState {
                report_id: parse_report_id(&values[0])?,
                state,
            })
        }
        "create-account" => {
            require_exact(command, values, 3)?;
            Ok(AdminRequest::CreateAccount {
                endpoint: parse_endpoint(&values[0])?,
                role: parse_role(&values[1])?,
                display_name: values[2].clone(),
            })
        }
        "list-endpoints" => {
            require_exact(command, values, 1)?;
            Ok(AdminRequest::ListEndpoints {
                account_id: parse_account_id(&values[0])?,
            })
        }
        "add-endpoint" | "revoke-endpoint" => {
            require_exact(command, values, 2)?;
            let account_id = parse_account_id(&values[0])?;
            let endpoint = parse_endpoint(&values[1])?;
            if command == "add-endpoint" {
                Ok(AdminRequest::AddEndpoint {
                    account_id,
                    endpoint,
                })
            } else {
                Ok(AdminRequest::RevokeEndpoint {
                    account_id,
                    endpoint,
                })
            }
        }
        _ => Err(format!(
            "unknown admin command: {command}\n{}",
            admin_usage()
        )),
    }
}

pub(crate) async fn run_account_key_operation(
    secret_key: SecretKey,
    profile: &Path,
    address_path: &Path,
    content: ContentIdentity,
    request: AccountKeyRequest,
) -> Result<String, Box<dyn std::error::Error>> {
    tokio::time::timeout(OPERATION_TIMEOUT, async {
        run_account_key_operation_inner(secret_key, profile, address_path, content, request).await
    })
    .await
    .map_err(|_| "account-key operation timed out")?
}

async fn run_account_key_operation_inner(
    secret_key: SecretKey,
    profile: &Path,
    address_path: &Path,
    content: ContentIdentity,
    request: AccountKeyRequest,
) -> Result<String, Box<dyn std::error::Error>> {
    let server_address: EndpointAddr = serde_json::from_slice(&std::fs::read(address_path)?)?;
    verify_existing_pin(profile, server_address.id)?;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .bind()
        .await?;
    let connection = endpoint.connect(server_address.clone(), GAME_ALPN).await?;
    require_datagram_support(&connection)?;
    if connection.remote_id() != server_address.id {
        return Err("iroh connected to an unexpected server identity".into());
    }
    pin_server(profile, server_address.id)?;
    let (mut send, mut receive) = connection.open_bi().await?;
    write_control_frame(
        &mut send,
        &ControlMessage::ClientHello(cdda_protocol::ClientHello {
            protocol_version: PROTOCOL_VERSION,
            content: content.clone(),
        }),
    )
    .await?;
    match read_control_frame(&mut receive).await? {
        ControlMessage::ServerHello(hello)
            if hello.protocol_version == PROTOCOL_VERSION && hello.content == content => {}
        ControlMessage::GameplayRejected(reason) => {
            return Err(format!("gameplay handshake rejected: {reason:?}").into());
        }
        _ => return Err("server returned an invalid gameplay hello".into()),
    }
    if !matches!(
        read_control_frame(&mut receive).await?,
        ControlMessage::CharacterList(_)
    ) {
        return Err("server omitted the character list".into());
    }
    write_control_frame(&mut send, &ControlMessage::AccountKeyRequest(request)).await?;
    let response = match read_control_frame(&mut receive).await? {
        ControlMessage::AccountKeyResponse(response) => response,
        _ => return Err("server returned an invalid account-key response".into()),
    };
    connection.close(0_u32.into(), b"account-key response received");
    endpoint.close().await;
    format_account_key_response(response).map_err(Into::into)
}

pub(crate) async fn run_admin_operation(
    secret_key: SecretKey,
    profile: &Path,
    address_path: &Path,
    request: AdminRequest,
) -> Result<String, Box<dyn std::error::Error>> {
    tokio::time::timeout(OPERATION_TIMEOUT, async {
        run_admin_operation_inner(secret_key, profile, address_path, request).await
    })
    .await
    .map_err(|_| "admin operation timed out")?
}

async fn run_admin_operation_inner(
    secret_key: SecretKey,
    profile: &Path,
    address_path: &Path,
    request: AdminRequest,
) -> Result<String, Box<dyn std::error::Error>> {
    let server_address: EndpointAddr = serde_json::from_slice(&std::fs::read(address_path)?)?;
    verify_existing_pin(profile, server_address.id)?;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .bind()
        .await?;
    let connection = endpoint.connect(server_address.clone(), ADMIN_ALPN).await?;
    if connection.remote_id() != server_address.id {
        return Err("iroh connected to an unexpected server identity".into());
    }
    pin_server(profile, server_address.id)?;
    let (mut send, mut receive) = connection.open_bi().await?;
    write_control_frame(
        &mut send,
        &ControlMessage::AdminHello(AdminHello {
            protocol_version: PROTOCOL_VERSION,
        }),
    )
    .await?;
    match read_control_frame(&mut receive).await? {
        ControlMessage::AdminResponse(AdminResponse::Ready { .. }) => {}
        ControlMessage::AdminResponse(AdminResponse::Rejected(reason)) => {
            return Err(format!("admin handshake rejected: {reason:?}").into());
        }
        _ => return Err("server returned an invalid admin hello".into()),
    }
    write_control_frame(&mut send, &ControlMessage::AdminRequest(request)).await?;
    let response = match read_control_frame(&mut receive).await? {
        ControlMessage::AdminResponse(response) => response,
        _ => return Err("server returned an invalid admin response".into()),
    };
    connection.close(0_u32.into(), b"admin response received");
    endpoint.close().await;
    format_admin_response(response).map_err(Into::into)
}

fn parse_endpoint(value: &str) -> Result<EndpointIdentity, String> {
    EndpointId::from_str(value)
        .map(|endpoint| EndpointIdentity(*endpoint.as_bytes()))
        .map_err(|error| format!("invalid iroh endpoint ID: {error}"))
}

fn parse_account_id(value: &str) -> Result<AccountId, String> {
    parse_stable_id(value, "account").map(|(namespace, counter)| AccountId::new(namespace, counter))
}

fn parse_actor_id(value: &str) -> Result<ActorId, String> {
    parse_stable_id(value, "actor").map(|(namespace, counter)| ActorId::new(namespace, counter))
}

fn parse_item_id(value: &str) -> Result<ItemId, String> {
    parse_stable_id(value, "item").map(|(namespace, counter)| ItemId::new(namespace, counter))
}

fn parse_stable_id(value: &str, kind: &str) -> Result<(u64, u64), String> {
    let (namespace, counter) = value
        .split_once(':')
        .ok_or_else(|| format!("{kind} ID must use NAMESPACE:COUNTER hexadecimal form"))?;
    if namespace.len() != 16 || counter.len() != 16 {
        return Err(format!(
            "{kind} ID components must each contain 16 hexadecimal digits"
        ));
    }
    let namespace = u64::from_str_radix(namespace, 16)
        .map_err(|_| format!("{kind} ID namespace is not hexadecimal"))?;
    let counter = u64::from_str_radix(counter, 16)
        .map_err(|_| format!("{kind} ID counter is not hexadecimal"))?;
    if namespace == 0 || counter == 0 {
        return Err(format!("{kind} ID components must be nonzero"));
    }
    Ok((namespace, counter))
}

fn parse_report_id(value: &str) -> Result<ReportId, String> {
    let value = parse_u64(value, "report ID")?;
    if value == 0 {
        return Err(String::from("report ID must be nonzero"));
    }
    Ok(ReportId(value))
}

fn parse_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{label} must be an unsigned decimal integer"))
}

fn parse_role(value: &str) -> Result<AccountRole, String> {
    match value {
        "player" => Ok(AccountRole::Player),
        "moderator" => Ok(AccountRole::Moderator),
        "administrator" => Ok(AccountRole::Administrator),
        _ => Err(format!("unknown account role: {value}")),
    }
}

fn parse_status(value: &str) -> Result<AccountStatus, String> {
    match value {
        "enabled" => Ok(AccountStatus::Enabled),
        "disabled" => Ok(AccountStatus::Disabled),
        "banned" => Ok(AccountStatus::Banned),
        _ => Err(format!(
            "account status must be enabled, disabled, or banned, not {value}"
        )),
    }
}

fn parse_report_filter(value: &str) -> Result<Option<ReportState>, String> {
    match value {
        "all" => Ok(None),
        "open" => Ok(Some(ReportState::Open)),
        "actioned" => Ok(Some(ReportState::Actioned)),
        "dismissed" => Ok(Some(ReportState::Dismissed)),
        _ => Err(format!(
            "report filter must be all, open, actioned, or dismissed, not {value}"
        )),
    }
}

fn parse_duration(value: &str) -> Result<Option<u32>, String> {
    if value == "off" {
        return Ok(None);
    }
    let seconds: u32 = value
        .parse()
        .map_err(|_| String::from("duration must be off or an unsigned number of seconds"))?;
    if seconds == 0 || seconds > MAX_MODERATION_DURATION_SECONDS {
        return Err(format!(
            "duration must be 1-{MAX_MODERATION_DURATION_SECONDS} seconds"
        ));
    }
    Ok(Some(seconds))
}

fn parse_limit(value: Option<&String>, maximum: u16, label: &str) -> Result<u16, String> {
    let Some(value) = value else {
        return Ok(maximum);
    };
    let limit: u16 = value
        .parse()
        .map_err(|_| format!("{label} limit must be an unsigned integer"))?;
    if limit == 0 || limit > maximum {
        return Err(format!("{label} limit must be 1-{maximum}"));
    }
    Ok(limit)
}

fn require_exact(command: &str, values: &[String], count: usize) -> Result<(), String> {
    require_range(command, values, count, count)
}

fn require_range(
    command: &str,
    values: &[String],
    minimum: usize,
    maximum: usize,
) -> Result<(), String> {
    if (minimum..=maximum).contains(&values.len()) {
        Ok(())
    } else {
        Err(format!(
            "admin command {command} received {} argument(s); expected {minimum}-{maximum}\n{}",
            values.len(),
            admin_usage()
        ))
    }
}

fn admin_usage() -> String {
    String::from(
        "admin commands: list-accounts [AFTER LIMIT]; list-characters ACCOUNT; inspect-character ACTOR [INVENTORY_AFTER INVENTORY_LIMIT]; list-reports [all|open|actioned|dismissed [AFTER LIMIT]]; history ACCOUNT [AFTER LIMIT]; role ACCOUNT ROLE; status ACCOUNT STATUS; suspend ACCOUNT off|SECONDS; mute ACCOUNT off|SECONDS; kick ACCOUNT; transfer ACTOR NEW_ACCOUNT; resolve-report REPORT actioned|dismissed; create-account ENDPOINT ROLE DISPLAY_NAME; list-endpoints ACCOUNT; add-endpoint ACCOUNT ENDPOINT; revoke-endpoint ACCOUNT ENDPOINT",
    )
}

fn endpoint_text(endpoint: EndpointIdentity) -> String {
    EndpointId::from_bytes(&endpoint.0)
        .map(|endpoint| endpoint.to_string())
        .unwrap_or_else(|_| format!("invalid:{:02x?}", endpoint.0))
}

fn binding_text(binding: EndpointBindingSummary) -> String {
    format!(
        "{} {:?} pending_expires_utc={:?}",
        endpoint_text(binding.endpoint),
        binding.state,
        binding.pending_expires_utc
    )
}

fn account_text(account: &cdda_protocol::AdminAccountSummary) -> String {
    format!(
        "{} {:?} {:?} display={:?} suspended_until_utc={:?} muted_until_utc={:?}",
        account.account_id,
        account.role,
        account.status,
        account.display_name,
        account.suspended_until_utc,
        account.muted_until_utc
    )
}

fn optional_text<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| String::from("none"), |value| value.to_string())
}

fn format_account_key_response(response: AccountKeyResponse) -> Result<String, String> {
    match response {
        AccountKeyResponse::Bindings(bindings) => Ok(if bindings.is_empty() {
            String::from("No endpoint bindings.")
        } else {
            bindings
                .into_iter()
                .map(binding_text)
                .collect::<Vec<_>>()
                .join("\n")
        }),
        AccountKeyResponse::Pending(binding) => Ok(format!(
            "Pending endpoint created. Prove it with --enroll-address before expiry.\n{}",
            binding_text(binding)
        )),
        AccountKeyResponse::Revoked { endpoint } => {
            Ok(format!("Revoked endpoint {}.", endpoint_text(endpoint)))
        }
        AccountKeyResponse::Rejected(reason) => {
            Err(format!("account-key operation rejected: {reason:?}"))
        }
    }
}

fn format_admin_response(response: AdminResponse) -> Result<String, String> {
    match response {
        AdminResponse::Ready { .. } => Err(String::from("server returned an extra admin hello")),
        AdminResponse::Rejected(reason) => Err(format!("admin operation rejected: {reason:?}")),
        AdminResponse::Accounts {
            accounts,
            next_after,
        } => {
            let mut lines = accounts.iter().map(account_text).collect::<Vec<_>>();
            lines.push(format!("next_after={}", optional_text(next_after)));
            Ok(lines.join("\n"))
        }
        AdminResponse::AccountUpdated(account) => Ok(format!("Updated {}", account_text(&account))),
        AdminResponse::Characters {
            account_id,
            characters,
            gameplay_session_active,
            controlled_actor,
        } => {
            let mut lines = vec![format!(
                "account={account_id} gameplay_session_active={gameplay_session_active} controlled_actor={}",
                optional_text(controlled_actor)
            )];
            lines.extend(
                characters
                    .into_iter()
                    .map(|character| format!("{} name={:?}", character.actor_id, character.name)),
            );
            Ok(lines.join("\n"))
        }
        AdminResponse::PrivateCharacter(character) => {
            let mut lines = vec![format!(
                "tick={} account={} actor={} name={:?} position=({},{},{}) hp={} base_strength={} base_dexterity={} base_intelligence={} base_perception={} connected={} wielded={} stored_kcal={} thirst={} sleepiness={} sleeping={} sleep_intervals={} speed={} action_points={} queued_actions={:?} craft_activity={:?} read_activity={:?} disassembly_activity={:?} learned_recipe_count={} skills={:?} inventory_total={} map_memory_chunks={} last_command_sequence={} last_held_input_sequence={} held_movement={:?}",
                character.tick.0,
                character.account_id,
                character.actor_id,
                character.name,
                character.position.x,
                character.position.y,
                character.position.z,
                character.hp,
                character.base_strength,
                character.base_dexterity,
                character.base_intelligence,
                character.base_perception,
                character.connected,
                optional_text(character.wielded),
                character.stored_kcal,
                character.thirst,
                character.sleepiness,
                character.sleeping,
                character.sleep_intervals,
                character.speed,
                character.action_points,
                character.queued_actions,
                character.craft_activity,
                character.read_activity,
                character.disassembly_activity,
                character.learned_recipe_count,
                character.skills,
                character.inventory_total,
                character.map_memory_chunks,
                character.last_command_sequence.0,
                character.last_held_input_sequence.0,
                character.held_movement,
            )];
            lines.extend(character.inventory.into_iter().map(|item| {
                format!(
                    "item={} type={:?} charges={} damage={} melee_damage_milli={:?} calories={} quench={} ammunition_type={:?} ranged={:?}",
                    item.id,
                    item.type_id,
                    item.charges,
                    item.damage,
                    item.melee_damage_milli,
                    item.calories,
                    item.quench,
                    item.ammunition_type,
                    item.ranged_weapon,
                )
            }));
            lines.push(format!(
                "next_inventory_after={}",
                optional_text(character.next_inventory_after)
            ));
            Ok(lines.join("\n"))
        }
        AdminResponse::Reports {
            reports,
            next_after,
        } => {
            let mut lines = reports.into_iter().map(|report| format!(
                "report={} state={:?} created_utc={} reporter={} actor={} character={:?} target={} actor={} character={:?} reason={:?} details={:?} resolved_utc={:?} resolved_by={} audit_sequence={:?}",
                report.report_id.0, report.state, report.created_utc, report.reporter_account,
                report.reporter_actor, report.reporter_character, report.target_account,
                report.target_actor, report.target_character, report.reason, report.details,
                report.resolved_utc, optional_text(report.resolved_by_account), report.resolution_audit_sequence
            )).collect::<Vec<_>>();
            lines.push(format!(
                "next_after={}",
                next_after.map_or_else(|| String::from("none"), |value| value.0.to_string())
            ));
            Ok(lines.join("\n"))
        }
        AdminResponse::ReportUpdated(report) => Ok(format!(
            "Updated report {} to {:?}; resolved_utc={:?} resolved_by={} audit_sequence={:?}",
            report.report_id.0,
            report.state,
            report.resolved_utc,
            optional_text(report.resolved_by_account),
            report.resolution_audit_sequence
        )),
        AdminResponse::AccountCreated {
            account,
            pending_endpoint,
        } => Ok(format!(
            "Created {}\nPending endpoint: {}",
            account_text(&account),
            binding_text(pending_endpoint)
        )),
        AdminResponse::Endpoints {
            account_id,
            bindings,
        } => {
            let mut lines = vec![format!("account={account_id}")];
            lines.extend(bindings.into_iter().map(binding_text));
            Ok(lines.join("\n"))
        }
        AdminResponse::EndpointPending {
            account_id,
            binding,
        } => Ok(format!(
            "Pending endpoint for account {account_id}. Prove it with --enroll-address before expiry.\n{}",
            binding_text(binding)
        )),
        AdminResponse::EndpointRevoked {
            account_id,
            endpoint,
        } => Ok(format!(
            "Revoked endpoint {} from account {account_id}.",
            endpoint_text(endpoint)
        )),
        AdminResponse::ModerationHistory {
            account_id,
            entries,
            next_after,
        } => {
            let mut lines = vec![format!("account={account_id}")];
            lines.extend(entries.into_iter().map(|entry| {
                format!(
                    "history={} audit={} occurred_utc={} operator={} kind={:?} until_utc={:?}",
                    entry.history_id,
                    entry.security_audit_sequence,
                    entry.occurred_utc,
                    entry.operator_account,
                    entry.kind,
                    entry.until_utc
                )
            }));
            lines.push(format!(
                "next_after={}",
                next_after.map_or_else(|| String::from("none"), |value| value.to_string())
            ));
            Ok(lines.join("\n"))
        }
        AdminResponse::ModerationApplied {
            account,
            kind,
            until_utc,
        } => Ok(format!(
            "Applied {kind:?} to {}; until_utc={until_utc:?}",
            account_text(&account)
        )),
        AdminResponse::CharacterTransferred {
            actor_id,
            previous_owner,
            new_owner,
        } => Ok(format!(
            "Transferred actor {actor_id} from {previous_owner} to {new_owner}."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| String::from(*value)).collect()
    }

    #[test]
    fn parses_every_admin_command_family() {
        let account = "0000000000000001:0000000000000002";
        let actor = "0000000000000001:0000000000000003";
        let item = "0000000000000001:0000000000000004";
        let endpoint = SecretKey::generate().public().to_string();
        let cases = [
            values(&["list-accounts"]),
            values(&["list-characters", account]),
            values(&["inspect-character", actor, item, "4"]),
            values(&["list-reports", "open", "1", "8"]),
            values(&["history", account, "1", "8"]),
            values(&["role", account, "moderator"]),
            values(&["status", account, "disabled"]),
            values(&["suspend", account, "60"]),
            values(&["mute", account, "off"]),
            values(&["kick", account]),
            values(&["transfer", actor, account]),
            values(&["resolve-report", "1", "dismissed"]),
            vec![
                String::from("create-account"),
                endpoint.clone(),
                String::from("player"),
                String::from("New Player"),
            ],
            values(&["list-endpoints", account]),
            vec![
                String::from("add-endpoint"),
                String::from(account),
                endpoint.clone(),
            ],
            vec![
                String::from("revoke-endpoint"),
                String::from(account),
                endpoint,
            ],
        ];
        for case in cases {
            parse_admin_operation(&case).expect("documented admin command should parse");
        }
    }

    #[test]
    fn rejects_ambiguous_or_out_of_bounds_admin_arguments() {
        let account = "0000000000000001:0000000000000002";
        assert!(parse_admin_operation(&values(&["list-reports", "invalid"])).is_err());
        assert!(parse_admin_operation(&values(&["suspend", account, "86401"])).is_err());
        assert!(parse_admin_operation(&values(&["kick", account, "extra"])).is_err());
        assert!(parse_admin_operation(&values(&["status", account, "recovery-locked"])).is_err());
    }

    #[test]
    fn private_character_output_exposes_canonical_base_stats() {
        let output = format_admin_response(AdminResponse::PrivateCharacter(Box::new(
            cdda_protocol::PrivateCharacterInspection {
                tick: cdda_protocol::SimTick(10),
                account_id: AccountId::new(1, 2),
                actor_id: ActorId::new(1, 3),
                name: String::from("Survivor"),
                position: cdda_protocol::WorldPosition { x: 1, y: 2, z: 0 },
                hp: 100,
                base_strength: 12,
                base_dexterity: 11,
                base_intelligence: 10,
                base_perception: 9,
                connected: true,
                last_command_sequence: cdda_protocol::CommandSequence(0),
                last_held_input_sequence: cdda_protocol::HeldInputSequence(0),
                held_movement: None,
                wielded: None,
                stored_kcal: 55_000,
                thirst: 0,
                sleepiness: 0,
                sleeping: false,
                sleep_intervals: 0,
                speed: 100,
                action_points: 0,
                queued_actions: Vec::new(),
                craft_activity: None,
                read_activity: None,
                disassembly_activity: None,
                construction_activity: None,
                learned_recipe_count: 0,
                skills: Vec::new(),
                proficiencies: Vec::new(),
                inventory_total: 0,
                inventory: Vec::new(),
                next_inventory_after: None,
                map_memory_chunks: 0,
            },
        )))
        .expect("private character response should format");
        assert!(output.contains(
            "hp=100 base_strength=12 base_dexterity=11 base_intelligence=10 base_perception=9 connected=true"
        ));
    }
}
