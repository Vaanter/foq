#[allow(unused)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum ConnectionMode {
    ACTIVE,
    #[default]
    PASSIVE,
}
