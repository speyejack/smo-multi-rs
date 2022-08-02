use anyhow::Result;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use crate::{cmds::Command, settings::SyncSettings};

pub struct Server {
    pub to_coord: mpsc::Sender<Command>,
    pub settings: SyncSettings,
}

impl Server {
    pub async fn listen_for_clients(self) -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        loop {
            let (socket, _) = listener.accept().await?;

            let to_coord = self.to_coord.clone();
            let settings = self.settings.clone();

            tokio::spawn(async move {
                Self::handle_new_client(socket, to_coord, settings).await;

                // let mut buffer = [0; 1024];
                // println!("Connection started");
                // socket.read(&mut buffer).await;
                // time::sleep(Duration::from_secs(3)).await;

                // socket.write_all(b"hello! :)").await;

                println!("Connection happened!");
            });
        }
    }

    async fn handle_new_client(
        socket: TcpStream,
        to_coord: mpsc::Sender<Command>,
        settings: SyncSettings,
    ) -> Result<()> {
        todo!()
        // let cli = Client::initialize_client(socket, to_coord, settings)?;

        // to_coord
        //     .send(Command::Server(ServerCommand::NewPlayer {
        //         guid,
        //         cli,
        //         comm: to_cli,
        //     }))
        //     .await?;

        // tokio::spawn(async move { cli.handle_events() });

        // Ok(())
    }
}
