use super::client::SshClient;

pub struct TmuxHelper<'a> {
    client: &'a SshClient,
    container_name: &'a str,
    session: &'a str,
}

impl<'a> TmuxHelper<'a> {
    pub fn new(client: &'a SshClient, container_name: &'a str, session: &'a str) -> Self {
        TmuxHelper {
            client,
            container_name,
            session,
        }
    }

    pub async fn send_keys(&self, keys: &str) -> anyhow::Result<()> {
        let cmd = format!(
            "docker exec {} tmux send-keys -t {} '{}' Enter",
            self.container_name,
            self.session,
            keys.replace('\'', "\\'")
        );
        self.client.execute(&cmd).await?;
        Ok(())
    }

    pub async fn capture_pane(&self) -> anyhow::Result<String> {
        let cmd = format!(
            "docker exec {} tmux capture-pane -p -J -e -t {} 2>/dev/null || echo ''",
            self.container_name, self.session
        );
        self.client.execute(&cmd).await
    }

    pub async fn new_session(&self) -> anyhow::Result<()> {
        let cmd = format!(
            "docker exec {} tmux new-session -d -s {} 2>/dev/null || true",
            self.container_name, self.session
        );
        self.client.execute(&cmd).await?;
        Ok(())
    }

    pub async fn has_session(&self) -> anyhow::Result<bool> {
        let cmd = format!(
            "docker exec {} tmux has-session -t {} 2>/dev/null && echo 'yes' || echo 'no'",
            self.container_name, self.session
        );
        let output = self.client.execute(&cmd).await?;
        Ok(output.trim() == "yes")
    }
}
