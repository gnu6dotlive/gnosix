use super::*;

#[test]
fn every_imported_icon_parses_and_stays_near_the_grid() {
    for style in [Style::Outline, Style::Filled] {
        let icons = all(style);
        assert!(icons.len() > 1000, "{style:?}: {}", icons.len());
        assert!(
            icons.windows(2).all(|w| w[0].name < w[1].name),
            "sorted, unique"
        );
        for icon in icons {
            let segments = parse(icon.path).unwrap_or_else(|e| panic!("{}: {e:?}", icon.name));
            assert!(!segments.is_empty(), "{}", icon.name);
            for s in &segments {
                let p = match *s {
                    Segment::MoveTo(p) | Segment::LineTo(p) => p,
                    Segment::Quad { to, .. }
                    | Segment::Cubic { to, .. }
                    | Segment::Arc { to, .. } => to,
                    Segment::Close => continue,
                };
                assert!(
                    (-1.0..=25.0).contains(&p.x) && (-1.0..=25.0).contains(&p.y),
                    "{} leaves the grid at {p:?}",
                    icon.name
                );
            }
        }
    }
}

#[test]
fn lookup_by_name_and_search_by_tag() {
    let power = icon(Style::Outline, "power").expect("power");
    assert_eq!(power.category, "Devices");
    assert!(power.tags().any(|t| t == "shutdown"));
    assert!(icon(Style::Outline, "no-such-icon").is_none());
    assert!(icon(Style::Filled, "bell").is_some());
    assert!(search(Style::Outline, "shutdown").any(|i| i.name == "power"));
}

#[test]
fn the_grammar_handles_relative_smooth_and_compact_forms() {
    // Relative lines and h/v, close.
    let s = parse("M2 2h4v4l-4 0z").unwrap();
    assert_eq!(s[1], Segment::LineTo(Point::new(6.0, 2.0)));
    assert_eq!(s[2], Segment::LineTo(Point::new(6.0, 6.0)));
    assert_eq!(s[3], Segment::LineTo(Point::new(2.0, 6.0)));
    assert_eq!(s[4], Segment::Close);
    // Implicit lines after a move, compact numbers.
    let s = parse("m1.5.5 1-1-2,2").unwrap();
    assert_eq!(s[0], Segment::MoveTo(Point::new(1.5, 0.5)));
    assert_eq!(s[1], Segment::LineTo(Point::new(2.5, -0.5)));
    assert_eq!(s[2], Segment::LineTo(Point::new(0.5, 1.5)));
    // Smooth cubic reflects the previous control point.
    let s = parse("M0 0C0 1 1 2 2 2s2-1 2-2").unwrap();
    assert_eq!(
        s[2],
        Segment::Cubic {
            ctrl1: Point::new(3.0, 2.0),
            ctrl2: Point::new(4.0, 1.0),
            to: Point::new(4.0, 0.0)
        }
    );
    // Arc flags glued to the coordinates.
    let s = parse("M7 6a7.75 7.75 0 1 0 10 0").unwrap();
    assert!(matches!(
        s[1],
        Segment::Arc {
            large_arc: true,
            sweep: false,
            ..
        }
    ));
    let s = parse("M0 0a1 1 0 0110 0").unwrap();
    assert!(
        matches!(s[1], Segment::Arc { large_arc: false, sweep: true, to, .. } if to == Point::new(10.0, 0.0))
    );
    assert!(parse("M0 0 X").is_err());
}

#[test]
fn length_measures_lines_and_arcs() {
    assert!((length(&parse("M0 0h3v4").unwrap()) - 7.0).abs() < 1e-4);
    // A half circle of radius 5.
    let half = length(&parse("M0 0a5 5 0 0 1 10 0").unwrap());
    assert!((half - std::f32::consts::PI * 5.0).abs() < 0.1, "{half}");
}

#[test]
fn flags_by_name_or_iso_code() {
    let fr = flag("FR").expect("France by ISO code");
    assert_eq!(fr.name, "france");
    assert_eq!(flag("canada").unwrap().iso, "CA");
    assert!(fr.svg.starts_with(b"<svg"));
    assert!(all_flags().len() > 250);
}
