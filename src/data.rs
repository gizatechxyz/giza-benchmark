pub(crate) struct Benchmark {
    pub(crate) runner: Option<Metrics>,
    pub(crate) prover: Metrics,
    pub(crate) verifier: Metrics,
}

pub(crate) struct Metrics {
    pub(crate) exec_time: f64,
    pub(crate) memory_usage: u64,
}
