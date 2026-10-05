//! Transport-independent GNOS.UX -> GNOSIX compositor intent.
//!
//! GNOS.UX owns resolved intent semantics. Application, fencing and compositor
//! observation remain outside this module.

use crate::{
    envelope::{
        CompositorIntentDefaults, KeyboardIntent, LogicalPx, RuntimeId, SpaceReservationIntent,
        SurfaceEdge,
    },
    scene::{MonitorSceneId, SceneEpoch, SceneFrame, SceneReservationIntent},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositorIntentSource {
    pub scene_id: MonitorSceneId,
    pub scene_epoch: SceneEpoch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCompositorIntent {
    pub source: CompositorIntentSource,
    pub edge: SurfaceEdge,
    pub keyboard: KeyboardIntent,
    pub reservation: SpaceReservationIntent,
    pub margins: [LogicalPx; 4],
    pub output: Option<RuntimeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositorIntentError {
    InvalidReservation,
    MultipleReservationEdges,
}
impl ResolvedCompositorIntent {
    pub fn from_envelope(
        source: CompositorIntentSource,
        compositor: &CompositorIntentDefaults,
    ) -> Self {
        Self {
            source,
            edge: compositor.edge,
            keyboard: compositor.keyboard,
            reservation: compositor.reservation,
            margins: compositor.margins,
            output: compositor.output.clone(),
        }
    }

    pub fn try_from_scene_frame(frame: &SceneFrame) -> Result<Self, CompositorIntentError> {
        Self::try_from_scene_frame_for_output(frame, None)
    }

    pub fn try_from_scene_frame_for_output(
        frame: &SceneFrame,
        output: Option<RuntimeId>,
    ) -> Result<Self, CompositorIntentError> {
        let (edge, reservation) = resolve_scene_reservation(frame.reservation)?;
        Ok(Self {
            source: CompositorIntentSource {
                scene_id: frame.scene_id,
                scene_epoch: frame.epoch,
            },
            edge,
            keyboard: KeyboardIntent::None,
            reservation,
            margins: [LogicalPx(0); 4],
            output,
        })
    }
}

fn resolve_scene_reservation(
    reservation: SceneReservationIntent,
) -> Result<(SurfaceEdge, SpaceReservationIntent), CompositorIntentError> {
    let candidates = [
        (SurfaceEdge::Top, reservation.top),
        (SurfaceEdge::Right, reservation.right),
        (SurfaceEdge::Bottom, reservation.bottom),
        (SurfaceEdge::Left, reservation.left),
    ];
    if candidates
        .iter()
        .any(|(_, value)| !value.is_finite() || *value < 0.0)
    {
        return Err(CompositorIntentError::InvalidReservation);
    }

    let active = candidates
        .iter()
        .copied()
        .filter(|(_, value)| *value > 0.5)
        .collect::<Vec<_>>();

    match active.as_slice() {
        [] => Ok((SurfaceEdge::Top, SpaceReservationIntent::Overlay)),
        [(edge, value)] => {
            let rounded = value.round();
            if rounded > u32::MAX as f32 {
                return Err(CompositorIntentError::InvalidReservation);
            }
            Ok((
                *edge,
                SpaceReservationIntent::Reserve(LogicalPx(rounded as u32)),
            ))
        }
        _ => Err(CompositorIntentError::MultipleReservationEdges),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{grammar::SurfaceViewport, scene::MonitorScene};

    #[test]
    fn scene_frame_resolves_to_typed_reservation() {
        let viewport = SurfaceViewport::try_new(1200.0, 720.0).expect("viewport");
        let mut scene = MonitorScene::new(MonitorSceneId::new(41), viewport).expect("scene");
        let frame = scene.tick(1.0 / 60.0).expect("frame");
        let intent = ResolvedCompositorIntent::try_from_scene_frame(&frame).expect("intent");

        assert_eq!(intent.source.scene_id, frame.scene_id);
        assert_eq!(intent.source.scene_epoch, frame.epoch);
        assert_eq!(intent.edge, SurfaceEdge::Top);
        assert!(matches!(
            intent.reservation,
            SpaceReservationIntent::Reserve(LogicalPx(44))
        ));
        assert_eq!(intent.output, None);
    }

    #[test]
    fn scene_frame_preserves_runtime_output_target() {
        let viewport = SurfaceViewport::try_new(1200.0, 720.0).expect("viewport");
        let mut scene = MonitorScene::new(MonitorSceneId::new(7), viewport).expect("scene");
        let frame = scene.tick(1.0 / 60.0).expect("frame");
        let output = RuntimeId::new("DP-2");
        let intent =
            ResolvedCompositorIntent::try_from_scene_frame_for_output(&frame, Some(output.clone()))
                .expect("intent");

        assert_eq!(intent.output, Some(output));
        assert_eq!(intent.source.scene_id, MonitorSceneId::new(7));
    }

    #[test]
    fn multiple_reservation_edges_are_rejected() {
        let reservation = SceneReservationIntent {
            top: 20.0,
            right: 0.0,
            bottom: 12.0,
            left: 0.0,
        };
        assert_eq!(
            resolve_scene_reservation(reservation),
            Err(CompositorIntentError::MultipleReservationEdges)
        );
    }

    #[test]
    fn zero_reservation_maps_to_overlay() {
        assert_eq!(
            resolve_scene_reservation(SceneReservationIntent::NONE),
            Ok((SurfaceEdge::Top, SpaceReservationIntent::Overlay))
        );
    }
}
