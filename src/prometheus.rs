use std::fmt::{self, Write};

#[derive(Debug)]
enum MetricType {
    Gauge,
}

#[derive(Debug)]
struct MetricLabel {
    name: String,
    value: String,
}

#[derive(Debug)]
struct MetricValue {
    label: Option<MetricLabel>,
    value: f64,
}

#[derive(Debug)]
pub struct Metric {
    t: MetricType,
    name: String,
    values: Vec<MetricValue>,
}

// TODO: validation
impl Metric {
    pub fn new_gauge<S: Into<String>>(name: S, value: f64) -> Metric {
        let value = MetricValue { label: None, value };
        Metric {
            t: MetricType::Gauge,
            name: name.into(),
            values: vec![value],
        }
    }

    pub fn new_gauge_with_label_values<S, V>(name: S, label_name: S, values: V) -> Metric
    where
        S: Into<String>,
        V: IntoIterator<Item = (String, f64)>,
    {
        let name = name.into();
        let label_name = label_name.into();

        let values = values.into_iter()
            .map(|(label_value, value)| {
                let label = Some (MetricLabel { name: label_name.clone(), value: label_value });
                MetricValue { label, value }
            })
            .collect();

        Metric {
            t: MetricType::Gauge,
            name,
            values,
        }
    }

    pub fn render<W: Write>(&self, sink: &mut W) -> fmt::Result {
        sink.write_fmt(format_args!("# HELP {}\n", self.name))?;

        let type_str = match self.t {
            MetricType::Gauge => "gauge"
        };
        sink.write_fmt(format_args!("# TYPE {} {}\n", self.name, type_str))?;

        for value in self.values.iter() {
            sink.write_str(&self.name)?;
            if let Some(label) = &value.label {
                sink.write_fmt(format_args!("{{{}=\"{}\"}}", label.name, label.value))?;
            }
            sink.write_fmt(format_args!(" {}\n", value.value))?;
        }

        Ok(())
    }
}

pub fn render_metrics<'a, M, W: Write>(metrics: M, sink: &mut W) -> fmt::Result
where
    M: IntoIterator<Item = &'a Metric>,
{
    for metric in metrics {
        metric.render(sink)?;
    }

    Ok(())
}
