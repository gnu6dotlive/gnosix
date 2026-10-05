use in_gnu_os_output_identity::{
    BindingConflictReason, BindingEvidence, BindingState, DurableOutputHubId,
    ExplicitBindingApproval, OutputIdentityRegistry, OutputObservation, RegistryError,
    UnboundReason,
};

fn id(byte: u8) -> DurableOutputHubId {
    DurableOutputHubId::from_bytes([byte; 16])
}

fn edid(byte: u8) -> BindingEvidence {
    BindingEvidence::EdidFingerprint([byte; 32])
}

fn approval() -> ExplicitBindingApproval {
    ExplicitBindingApproval::operator("operator-decision-test")
}

#[test]
fn a_known_output_remains_bound_after_registry_cold_start() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    assert!(matches!(
        registry.document().audit.first(),
        Some(in_gnu_os_output_identity::RegistryAuditEvent::HubCreated {
            authority_reference,
            ..
        }) if authority_reference == "test-provisioning"
    ));

    let observation = OutputObservation::new([edid(9)]).with_connector("DP-2");
    registry.bind(output, &observation, 1, approval()).unwrap();

    let persisted = registry.to_json().unwrap();
    let restarted = OutputIdentityRegistry::from_json(&persisted).unwrap();

    assert!(matches!(
        restarted.resolve(&observation),
        BindingState::Bound { output_hub_id, .. } if output_hub_id == output
    ));
}

#[test]
fn an_unknown_output_is_unbound_and_geometry_is_not_an_input_to_identity() {
    let registry = OutputIdentityRegistry::new();
    let observation = OutputObservation::new([edid(77)])
        .with_connector("DP-7")
        .with_make_model("Example", "Panel")
        .with_physical_size_mm(600, 340);

    assert_eq!(
        registry.resolve(&observation),
        BindingState::Unbound {
            reason: UnboundReason::NoMatchingBinding
        }
    );
}

#[test]
fn same_connector_replug_requires_the_stable_descriptor_to_resume_binding() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    let known = OutputObservation::new([edid(9)]).with_connector("DP-2");
    registry.bind(output, &known, 1, approval()).unwrap();

    let connector_only = OutputObservation::default().with_connector("DP-2");
    assert!(matches!(
        registry.resolve(&known),
        BindingState::Bound { output_hub_id, .. } if output_hub_id == output
    ));
    assert_eq!(
        registry.resolve(&connector_only),
        BindingState::DescriptorConflict {
            candidates: [output].into_iter().collect(),
            reason: BindingConflictReason::ConnectorHistoryIsNotIdentity,
        }
    );
}

#[test]
fn a_new_stable_descriptor_at_a_known_connector_requires_review() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    let known = OutputObservation::new([edid(9)]).with_connector("DP-2");
    registry.bind(output, &known, 1, approval()).unwrap();

    let replacement = OutputObservation::new([edid(10)]).with_connector("DP-2");
    assert_eq!(
        registry.resolve(&replacement),
        BindingState::DescriptorConflict {
            candidates: [output].into_iter().collect(),
            reason: BindingConflictReason::StrongDescriptorDisagreement,
        }
    );
}

#[test]
fn weak_descriptors_and_geometry_cannot_create_a_binding() {
    let registry = OutputIdentityRegistry::new();
    let observation = OutputObservation::default()
        .with_make_model("Example", "Panel")
        .with_physical_size_mm(600, 340);
    assert_eq!(
        registry.resolve(&observation),
        BindingState::Unbound {
            reason: UnboundReason::NoStableEvidence
        }
    );
}

#[test]
fn a_strong_descriptor_can_follow_a_connector_change() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    let first = OutputObservation::new([edid(9)]).with_connector("DP-2");
    registry.bind(output, &first, 1, approval()).unwrap();

    let moved = OutputObservation::new([edid(9)]).with_connector("HDMI-A-1");
    assert!(matches!(
        registry.resolve(&moved),
        BindingState::Bound { output_hub_id, .. } if output_hub_id == output
    ));
}

#[test]
fn strong_evidence_collision_in_loaded_data_is_ambiguous_not_guessed() {
    let mut registry = OutputIdentityRegistry::new();
    let first = id(1);
    registry
        .register_hub(first, 0, "test-provisioning")
        .unwrap();
    let known = OutputObservation::new([edid(9)]).with_connector("DP-2");
    registry.bind(first, &known, 1, approval()).unwrap();

    // A restored registry may contain historical/corrupt duplicate claims.
    // Loading must preserve that fact for fail-closed resolution, not pick one.
    let mut json: serde_json::Value = serde_json::from_str(&registry.to_json().unwrap()).unwrap();
    let second = id(2);
    json["output_hubs"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(second).unwrap());
    json["audit"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "kind": "hub_created",
            "revision": 3,
            "output_hub_id": serde_json::to_value(second).unwrap(),
            "authority_reference": "recovered-duplicate-claim"
        }));
    let mut duplicate = json["bindings"][0].clone();
    duplicate["record_id"] = serde_json::json!(2);
    duplicate["output_hub_id"] = serde_json::to_value(second).unwrap();
    duplicate["revision"] = serde_json::json!(4);
    duplicate["approval_reference"] = serde_json::json!("recovered-duplicate-claim");
    json["bindings"].as_array_mut().unwrap().push(duplicate);
    json["audit"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "kind": "bound",
            "revision": 4,
            "output_hub_id": serde_json::to_value(second).unwrap(),
            "record_id": 2,
            "approval_reference": "recovered-duplicate-claim"
        }));
    json["revision"] = serde_json::json!(4);
    json["next_record_id"] = serde_json::json!(3);
    let restored = OutputIdentityRegistry::from_json(&json.to_string()).unwrap();

    assert_eq!(
        restored.resolve(&known),
        BindingState::Ambiguous {
            candidates: [first, second].into_iter().collect(),
        }
    );
}

#[test]
fn resolving_ambiguity_requires_and_records_an_explicit_selection() {
    let mut registry = OutputIdentityRegistry::new();
    let first = id(1);
    let second = id(2);
    registry
        .register_hub(first, 0, "test-provisioning")
        .unwrap();
    registry
        .register_hub(second, 1, "test-provisioning")
        .unwrap();
    let known = OutputObservation::new([edid(9)]).with_connector("DP-2");
    registry.bind(first, &known, 2, approval()).unwrap();

    let mut json: serde_json::Value = serde_json::from_str(&registry.to_json().unwrap()).unwrap();
    let mut duplicate = json["bindings"][0].clone();
    duplicate["output_hub_id"] = serde_json::to_value(second).unwrap();
    duplicate["record_id"] = serde_json::json!(2);
    duplicate["revision"] = serde_json::json!(4);
    duplicate["approval_reference"] = serde_json::json!("recovered-duplicate-claim");
    json["bindings"].as_array_mut().unwrap().push(duplicate);
    json["audit"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "kind": "bound",
            "revision": 4,
            "output_hub_id": serde_json::to_value(second).unwrap(),
            "record_id": 2,
            "approval_reference": "recovered-duplicate-claim"
        }));
    json["revision"] = serde_json::json!(4);
    json["next_record_id"] = serde_json::json!(3);
    let mut registry = OutputIdentityRegistry::from_json(&json.to_string()).unwrap();
    let candidates = [first, second].into_iter().collect();

    registry
        .approve_ambiguous_binding(second, &known, &candidates, 4, approval())
        .unwrap();

    assert!(matches!(
        registry.resolve(&known),
        BindingState::Bound { output_hub_id, .. } if output_hub_id == second
    ));
    assert!(matches!(
        registry.document().audit.last(),
        Some(in_gnu_os_output_identity::RegistryAuditEvent::AmbiguityResolved {
            candidates: audited,
            selected_output_hub_id,
            retired_record_ids,
            ..
        }) if audited == &candidates
            && *selected_output_hub_id == second
            && retired_record_ids == &[1].into_iter().collect()
    ));
}

#[test]
fn contradictory_same_kind_descriptor_evidence_is_a_conflict() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    let known = OutputObservation::new([edid(9), BindingEvidence::SerialFingerprint([3; 32])])
        .with_make_model("Example", "Panel");
    registry.bind(output, &known, 1, approval()).unwrap();

    let contradictory =
        OutputObservation::new([edid(9), BindingEvidence::SerialFingerprint([4; 32])])
            .with_make_model("Example", "Panel");
    assert_eq!(
        registry.resolve(&contradictory),
        BindingState::DescriptorConflict {
            candidates: [output].into_iter().collect(),
            reason: BindingConflictReason::StrongDescriptorDisagreement,
        }
    );
}

#[test]
fn physical_descriptor_disagreement_is_reported_after_a_strong_match() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    let known = OutputObservation::new([edid(9)])
        .with_make_model("Example", "Panel")
        .with_physical_size_mm(600, 340);
    registry.bind(output, &known, 1, approval()).unwrap();

    let mismatch = OutputObservation::new([edid(9)])
        .with_make_model("Example", "Panel")
        .with_physical_size_mm(500, 300);
    assert_eq!(
        registry.resolve(&mismatch),
        BindingState::DescriptorConflict {
            candidates: [output].into_iter().collect(),
            reason: BindingConflictReason::StrongDescriptorDisagreement,
        }
    );
}

#[test]
fn explicit_rebind_retires_old_evidence_and_audits_the_replacement() {
    let mut registry = OutputIdentityRegistry::new();
    let old_hub = id(1);
    let new_hub = id(2);
    registry
        .register_hub(old_hub, 0, "test-provisioning")
        .unwrap();
    registry
        .register_hub(new_hub, 1, "test-provisioning")
        .unwrap();
    let old_observation = OutputObservation::new([edid(9)]).with_connector("DP-2");
    registry
        .bind(old_hub, &old_observation, 2, approval())
        .unwrap();
    let replacement = OutputObservation::new([edid(10)]).with_connector("DP-2");
    let before_invalid_bind = registry.to_json().unwrap();
    assert_eq!(
        registry.bind(old_hub, &replacement, 3, approval()),
        Err(RegistryError::HubAlreadyBound)
    );
    assert_eq!(registry.to_json().unwrap(), before_invalid_bind);

    registry
        .rebind(old_hub, new_hub, &replacement, 3, approval())
        .unwrap();
    let restarted = OutputIdentityRegistry::from_json(&registry.to_json().unwrap()).unwrap();

    assert_eq!(
        restarted.resolve(&old_observation),
        BindingState::DescriptorConflict {
            candidates: [old_hub].into_iter().collect(),
            reason: BindingConflictReason::PreviouslyBoundEvidenceWasRetired,
        }
    );
    assert!(matches!(
        restarted.resolve(&replacement),
        BindingState::Bound { output_hub_id, .. } if output_hub_id == new_hub
    ));
    assert!(matches!(
        registry.document().audit.last(),
        Some(in_gnu_os_output_identity::RegistryAuditEvent::Rebound {
            from_output_hub_id,
            to_output_hub_id,
            retired_record_ids,
            ..
        }) if *from_output_hub_id == old_hub
            && *to_output_hub_id == new_hub
            && retired_record_ids == &[1].into_iter().collect()
    ));
}

#[test]
fn rebind_cannot_assign_an_output_hub_that_is_already_bound() {
    let mut registry = OutputIdentityRegistry::new();
    let first = id(1);
    let second = id(2);
    registry
        .register_hub(first, 0, "test-provisioning")
        .unwrap();
    registry
        .register_hub(second, 1, "test-provisioning")
        .unwrap();
    let first_observation = OutputObservation::new([edid(9)]).with_connector("DP-2");
    let second_observation = OutputObservation::new([edid(10)]).with_connector("HDMI-A-1");
    registry
        .bind(first, &first_observation, 2, approval())
        .unwrap();
    registry
        .bind(second, &second_observation, 3, approval())
        .unwrap();
    let before = registry.to_json().unwrap();

    assert_eq!(
        registry.rebind(
            first,
            second,
            &OutputObservation::new([edid(11)]).with_connector("DP-2"),
            4,
            approval(),
        ),
        Err(RegistryError::HubAlreadyBound)
    );
    assert_eq!(registry.to_json().unwrap(), before);
}

#[test]
fn persisted_active_flags_must_match_the_audited_binding_history() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    registry
        .bind(
            output,
            &OutputObservation::new([edid(9)]).with_connector("DP-2"),
            1,
            approval(),
        )
        .unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&registry.to_json().unwrap()).unwrap();
    json["bindings"][0]["active"] = serde_json::json!(false);

    assert!(matches!(
        OutputIdentityRegistry::from_json(&json.to_string()),
        Err(RegistryError::InvalidDocument(_))
    ));
}

#[test]
fn stale_binding_command_is_rejected_without_mutating_registry() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    let before = registry.to_json().unwrap();

    assert_eq!(
        registry.register_hub(id(2), 0, "test-provisioning"),
        Err(RegistryError::StaleRevision {
            expected: 0,
            actual: 1
        })
    );
    assert_eq!(registry.to_json().unwrap(), before);
}

#[test]
fn an_output_without_stable_evidence_cannot_be_persistently_bound() {
    let mut registry = OutputIdentityRegistry::new();
    let output = id(1);
    registry
        .register_hub(output, 0, "test-provisioning")
        .unwrap();
    let before = registry.to_json().unwrap();
    let connector_only = OutputObservation::default().with_connector("DP-2");

    assert_eq!(
        registry.bind(output, &connector_only, 1, approval()),
        Err(RegistryError::MissingStableEvidence)
    );
    assert_eq!(registry.to_json().unwrap(), before);
}

#[test]
fn unknown_schema_version_fails_closed() {
    let mut registry = OutputIdentityRegistry::new();
    registry
        .register_hub(id(1), 0, "test-provisioning")
        .unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&registry.to_json().unwrap()).unwrap();
    json["schema_version"] = serde_json::json!(2);

    assert_eq!(
        OutputIdentityRegistry::from_json(&json.to_string()),
        Err(RegistryError::UnsupportedSchema(2))
    );
}

#[test]
fn incomplete_or_reordered_audit_history_fails_closed() {
    let mut registry = OutputIdentityRegistry::new();
    registry
        .register_hub(id(1), 0, "test-provisioning")
        .unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&registry.to_json().unwrap()).unwrap();
    json["audit"][0]["revision"] = serde_json::json!(0);

    assert!(matches!(
        OutputIdentityRegistry::from_json(&json.to_string()),
        Err(RegistryError::InvalidDocument(_))
    ));
}
