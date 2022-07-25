use crate::error::Result;
use crate::Client;
use argh::FromArgs;
use model::{Outcome, TestResults};

#[derive(Debug, FromArgs, PartialEq)]
#[argh(
    subcommand,
    name = "send-result",
    description = "send test result for every rerun"
)]
pub(crate) struct SendResults {
    #[argh(
        short = 'o',
        option,
        description = "outcome of result as in pass/fail/timeout/unknown"
    )]
    outcome: String,
    #[argh(short = 'p', option, description = "number of passed test cases")]
    passed: u64,
    #[argh(short = 'f', option, description = "number of failed test cases")]
    failed: u64,
    #[argh(short = 's', option, description = "number of skipped test cases")]
    skipped: u64,
    #[argh(option, description = "additional result information")]
    other_info: Option<String>,
}

impl SendResults {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let outcome: Outcome = serde_plain::from_str::<Outcome>(&self.outcome).unwrap();
        let test_results = TestResults {
            outcome,
            num_passed: self.passed,
            num_failed: self.failed,
            num_skipped: self.skipped,
            other_info: Some(
                self.other_info
                    .as_deref()
                    .unwrap_or("Test results saved")
                    .to_string(),
            ),
        };
        k8s_client.send_test_results(test_results).await?;
        Ok(())
    }
}
