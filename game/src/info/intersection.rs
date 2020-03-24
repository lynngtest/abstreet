use crate::app::App;
use crate::helpers::{rotating_color_map, ID};
use crate::info::{throughput, InfoTab};
use abstutil::prettyprint_usize;
use ezgui::{Btn, EventCtx, Line, Plot, PlotOptions, Series, Text, Widget};
use geom::{Duration, Statistic, Time};
use map_model::{IntersectionID, IntersectionType};
use sim::Analytics;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone)]
pub enum Tab {
    Throughput,
    Delay,
}

pub fn info(
    ctx: &EventCtx,
    app: &App,
    id: IntersectionID,
    tab: InfoTab,
    header_btns: Widget,
    action_btns: Vec<Widget>,
    hyperlinks: &mut HashMap<String, (ID, InfoTab)>,
) -> Vec<Widget> {
    let mut rows = vec![];

    let i = app.primary.map.get_i(id);

    let label = match i.intersection_type {
        IntersectionType::StopSign => format!("Intersection #{} (Stop signs)", id.0),
        IntersectionType::TrafficSignal => format!("Intersection #{} (Traffic signals)", id.0),
        IntersectionType::Border => format!("Border #{}", id.0),
        IntersectionType::Construction => format!("Intersection #{} (under construction)", id.0),
    };
    rows.push(Widget::row(vec![
        Line(label).roboto_bold().draw(ctx),
        header_btns,
    ]));

    let mut txt = Text::from(Line("Connecting"));
    let mut road_names = BTreeSet::new();
    for r in &i.roads {
        road_names.insert(app.primary.map.get_r(*r).get_name());
    }
    for r in road_names {
        // TODO The spacing is ignored, so use -
        txt.add(Line(format!("- {}", r)));
    }
    rows.push(txt.draw(ctx));

    // TODO Inactive
    // TODO Naming, style...
    rows.push(Widget::row(vec![
        Btn::text_bg2("Main").build_def(ctx, None),
        Btn::text_bg2("Throughput").build_def(ctx, None), // TODO temporary name
        if app.primary.map.get_i(id).is_traffic_signal() {
            Btn::text_bg2("Delay").build_def(ctx, None)
        } else {
            Widget::nothing()
        },
    ]));
    hyperlinks.insert(
        "Throughput".to_string(),
        (ID::Intersection(id), InfoTab::Intersection(Tab::Throughput)),
    );
    if app.primary.map.get_i(id).is_traffic_signal() {
        hyperlinks.insert(
            "Delay".to_string(),
            (ID::Intersection(id), InfoTab::Intersection(Tab::Delay)),
        );
    }

    match tab {
        InfoTab::Nil => {
            rows.extend(action_btns);

            let trip_lines = app.primary.sim.count_trips_involving_border(id).describe();
            if !trip_lines.is_empty() {
                let mut txt = Text::new();
                for line in trip_lines {
                    txt.add(Line(line));
                }
                rows.push(txt.draw(ctx));
            }
        }
        InfoTab::Intersection(Tab::Throughput) => {
            let mut txt = Text::new();

            txt.add(Line("Throughput").roboto_bold());
            txt.add(Line(format!(
                "Since midnight: {} agents crossed",
                prettyprint_usize(
                    app.primary
                        .sim
                        .get_analytics()
                        .thruput_stats
                        .count_per_intersection
                        .get(id)
                )
            )));
            txt.add(Line(format!("In 20 minute buckets:")));
            rows.push(txt.draw(ctx));

            rows.push(
                throughput(ctx, app, move |a, t| {
                    a.throughput_intersection(t, id, Duration::minutes(20))
                })
                .margin(10),
            );
        }
        InfoTab::Intersection(Tab::Delay) => {
            assert!(app.primary.map.get_i(id).is_traffic_signal());
            let mut txt = Text::from(Line("Delay").roboto_bold());
            txt.add(Line(format!("In 20 minute buckets:")));
            rows.push(txt.draw(ctx));

            rows.push(delay(ctx, app, id, Duration::minutes(20)).margin(10));
        }
        _ => unreachable!(),
    }

    rows
}

fn delay(ctx: &EventCtx, app: &App, i: IntersectionID, bucket: Duration) -> Widget {
    let get_data = |a: &Analytics, t: Time| {
        let mut series: Vec<(Statistic, Vec<(Time, Duration)>)> = Statistic::all()
            .into_iter()
            .map(|stat| (stat, Vec::new()))
            .collect();
        for (t, distrib) in a.intersection_delays_bucketized(t, i, bucket) {
            for (stat, pts) in series.iter_mut() {
                if distrib.count() == 0 {
                    pts.push((t, Duration::ZERO));
                } else {
                    pts.push((t, distrib.select(*stat)));
                }
            }
        }
        series
    };

    let mut all_series = Vec::new();
    for (idx, (stat, pts)) in get_data(app.primary.sim.get_analytics(), app.primary.sim.time())
        .into_iter()
        .enumerate()
    {
        all_series.push(Series {
            label: stat.to_string(),
            color: rotating_color_map(idx),
            pts,
        });
    }
    if app.has_prebaked().is_some() {
        for (idx, (stat, pts)) in get_data(app.prebaked(), Time::END_OF_DAY)
            .into_iter()
            .enumerate()
        {
            all_series.push(Series {
                label: format!("{} (baseline)", stat),
                color: rotating_color_map(idx).alpha(0.3),
                pts,
            });
        }
    }

    Plot::new_duration(ctx, all_series, PlotOptions::new())
}