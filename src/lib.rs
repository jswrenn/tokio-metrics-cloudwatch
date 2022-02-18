/// Samples and sends [`TaskMetrics`][tokio_metrics::TaskMetrics] from a given [`TaskMonitor`][tokio_metrics::TaskMonitor]
/// to CloudWatch at a given `frequency`.
pub async fn stream_task_metrics<N>(
    client: &aws_sdk_cloudwatch::Client,
    namespace: N,
    dimensions: Option<Vec<aws_sdk_cloudwatch::model::Dimension>>,
    monitor: tokio_metrics::TaskMonitor,
    frequency: std::time::Duration,
) -> Result<(), aws_sdk_cloudwatch::SdkError<aws_sdk_cloudwatch::error::PutMetricDataError>>
where
    N: Into<String>,
{
    let namespace = &namespace.into();
    for metrics in monitor.intervals() {
        let now = std::time::SystemTime::now();
        let timestamp = aws_sdk_cloudwatch::DateTime::from(now);
        send_task_metrics(client, namespace, dimensions.clone(), metrics, timestamp).await?;
        tokio::time::sleep(frequency).await;
    }
    Ok(())
}

/// Sends a given [`TaskMetrics`][tokio_metrics::TaskMetrics] to CloudWatch.
pub async fn send_task_metrics<N>(
    client: &aws_sdk_cloudwatch::Client,
    namespace: N,
    dimensions: Option<Vec<aws_sdk_cloudwatch::model::Dimension>>,
    metrics: tokio_metrics::TaskMetrics,
    timestamp: aws_sdk_cloudwatch::DateTime,
) -> Result<
    aws_sdk_cloudwatch::output::PutMetricDataOutput,
    aws_sdk_cloudwatch::SdkError<aws_sdk_cloudwatch::error::PutMetricDataError>,
>
where
    N: Into<String>,
{
    use aws_sdk_cloudwatch::model::StandardUnit::{Count, Microseconds, Percent};

    fn to_micros(duration: std::time::Duration) -> f64 {
        let micros = duration.as_secs() as f64 * 1_000_000.;
        let subsec_nanos = duration.subsec_nanos() as f64;
        let subsec_micros = subsec_nanos / 1_000.;
        micros + subsec_micros as f64
    }

    macro_rules! metric {
        ($name: ident, $unit: expr, $value: expr) => {
            aws_sdk_cloudwatch::model::MetricDatum::builder()
                .metric_name(stringify!($name))
                .timestamp(timestamp)
                .set_dimensions(dimensions.clone())
                .unit($unit)
                .values($value)
                .build()
        };
    }

    macro_rules! count {
        ( $name:ident ) => {
            metric!($name, Count, metrics.$name as f64)
        };
        ( $name:ident () ) => {
            metric!($name, Count, metrics.$name() as f64)
        };
    }

    macro_rules! duration {
        ( $name:ident ) => {
            metric!($name, Microseconds, to_micros(metrics.$name))
        };
        ( $name:ident () ) => {
            metric!($name, Microseconds, to_micros(metrics.$name()))
        };
    }

    macro_rules! percent {
        ( $name:ident ) => {
            metric!($name, Percent, metrics.$name as f64)
        };
        ( $name:ident () ) => {
            metric!($name, Percent, metrics.$name() as f64)
        };
    }

    client
        .put_metric_data()
        .namespace(namespace.into())
        // lifespan
        .metric_data(count!(instrumented_count))
        .metric_data(count!(dropped_count))
        // first polls
        .metric_data(count!(first_poll_count))
        .metric_data(duration!(total_first_poll_delay))
        .metric_data(duration!(mean_first_poll_delay()))
        // idles
        .metric_data(count!(total_idled_count))
        .metric_data(duration!(total_idle_duration))
        .metric_data(duration!(mean_idle_duration()))
        // schedules
        .metric_data(count!(total_scheduled_count))
        .metric_data(duration!(total_scheduled_duration))
        .metric_data(duration!(mean_scheduled_duration()))
        // polls
        .metric_data(count!(total_poll_count))
        .metric_data(duration!(total_poll_duration))
        .metric_data(duration!(mean_poll_duration()))
        .metric_data(percent!(slow_poll_ratio()))
        // fast polls
        .metric_data(count!(total_fast_poll_count))
        .metric_data(duration!(total_fast_poll_duration))
        .metric_data(duration!(mean_fast_poll_duration()))
        // slow polls
        .metric_data(count!(total_slow_poll_count))
        .metric_data(duration!(total_slow_poll_duration))
        .metric_data(duration!(mean_slow_poll_duration()))
        // send it!
        .send()
        .await
}
