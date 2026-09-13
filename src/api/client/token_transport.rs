use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
};

pub(super) fn agent(config: ureq::config::Config) -> ureq::Agent {
    ureq::Agent::with_parts(
        config,
        DefaultConnector::default().chain(TokenConnector),
        DefaultResolver::default(),
    )
}

#[derive(Debug)]
struct TokenConnector;

impl<T: Transport> Connector<T> for TokenConnector {
    type Out = TokenTransport<T>;

    fn connect(
        &self,
        _: &ConnectionDetails,
        chained: Option<T>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(TokenTransport))
    }
}

#[derive(Debug)]
struct TokenTransport<T>(T);

impl<T: Transport> Transport for TokenTransport<T> {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.0.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.0.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        loop {
            let Some(after) = super::first_token::read_timeout()? else {
                return self.0.await_input(timeout);
            };
            // Short polls enforce cancellation and the current first-output or
            // renewable output-idle deadline, not a whole-stream duration cap.
            let next = NextTimeout {
                after: after.into(),
                reason: ureq::Timeout::RecvBody,
            };
            match self.0.await_input(next) {
                Err(ureq::Error::Timeout(_)) => continue,
                Err(ureq::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue;
                }
                other => return other,
            }
        }
    }

    fn is_open(&mut self) -> bool {
        self.0.is_open()
    }
    fn is_tls(&self) -> bool {
        self.0.is_tls()
    }
}
