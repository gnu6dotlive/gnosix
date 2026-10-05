use bytemuck::{Pod, Zeroable};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameDemand {
    OnChange,
    Continuous,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameParams {
    pub primary: [f32; 4],
    pub control: [f32; 4],
}

pub trait PresentationSource {
    fn frame_demand(&self) -> FrameDemand;
    fn frame(&mut self, elapsed: Duration) -> FrameParams;
    fn label(&self) -> &'static str;
}

#[derive(Debug)]
pub struct SolidSource {
    color: [f32; 4],
}

impl SolidSource {
    pub fn new(color: [f32; 4]) -> Self {
        Self { color }
    }
}

impl PresentationSource for SolidSource {
    fn frame_demand(&self) -> FrameDemand {
        FrameDemand::OnChange
    }

    fn frame(&mut self, _elapsed: Duration) -> FrameParams {
        FrameParams {
            primary: self.color,
            control: [0.0, 0.0, 0.0, 0.0],
        }
    }

    fn label(&self) -> &'static str {
        "solid"
    }
}

#[derive(Debug, Default)]
pub struct ProceduralSource;

impl PresentationSource for ProceduralSource {
    fn frame_demand(&self) -> FrameDemand {
        FrameDemand::Continuous
    }

    fn frame(&mut self, elapsed: Duration) -> FrameParams {
        FrameParams {
            primary: [0.035, 0.045, 0.070, 1.0],
            control: [elapsed.as_secs_f32(), 1.0, 0.0, 0.0],
        }
    }

    fn label(&self) -> &'static str {
        "procedural"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_is_on_change_and_preserves_color() {
        let color = [0.1, 0.2, 0.3, 1.0];
        let mut source = SolidSource::new(color);

        assert_eq!(source.frame_demand(), FrameDemand::OnChange);
        assert_eq!(source.frame(Duration::from_secs(9)).primary, color);
        assert_eq!(source.frame(Duration::ZERO).control[1], 0.0);
    }

    #[test]
    fn procedural_is_continuous_and_encodes_time() {
        let mut source = ProceduralSource;

        assert_eq!(source.frame_demand(), FrameDemand::Continuous);
        let frame = source.frame(Duration::from_millis(1500));
        assert!((frame.control[0] - 1.5).abs() < f32::EPSILON);
        assert_eq!(frame.control[1], 1.0);
    }
}
