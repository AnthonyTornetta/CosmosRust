use bevy::{
    app::{App, Update},
    ecs::{
        event::{EventId, SendBatchIds},
        system::SystemParam,
    },
    log::error,
    prelude::{resource_exists, Deref, Event, EventReader, EventWriter, IntoSystemConfigs, OnEnter, Res, ResMut},
};
use renet2::{ClientId, RenetServer};

use crate::{
    netty::{cosmos_encoder, system_sets::NetworkingSystemsSet, NettyChannelClient},
    registry::identifiable::Identifiable,
};
use crate::{registry::Registry, state::GameState};

use super::netty_event::{EventReceiver, NettyEvent, NettyEventMessage, RegisteredNettyEvent};

#[derive(Event)]
pub(super) struct GotNetworkEvent {
    pub component_id: u16,
    pub raw_data: Vec<u8>,
    pub client_id: renet2::ClientId,
}

#[derive(Event, Debug)]
/// Send this event before the [`NetworkingSystemsSet::SyncComponents`] set to automatically have
/// the inner event sent to the client.
pub struct NettyEventToSend<T: NettyEvent> {
    /// The event to send
    pub event: T,
    /// The client to send this to or [`None`] to broadcast this to everyone.
    pub client_id: Option<ClientId>,
}

#[derive(Deref, Event, Debug)]
/// An event received from a client.
///
/// Read via [`EventReader<NettyEventReceived<T>>`]
pub struct NettyEventReceived<T: NettyEvent> {
    #[deref]
    /// The actual event received from the client
    pub event: T,
    /// The client that sent this event
    pub client_id: ClientId,
}

/// Send your [`NettyEvent`] via this before [`NetworkingSystemsSet::SyncComponents`] to have it
/// automatically sent to the server.
#[derive(SystemParam)]
pub struct NettyEventWriter<'w, T: NettyEvent> {
    ev_writer: EventWriter<'w, NettyEventToSend<T>>,
}

impl<'w, E: NettyEvent> NettyEventWriter<'w, E> {
    /// Sends an `event`, which can later be read by [`EventReader`]s.
    /// This method returns the [ID](`EventId`) of the sent `event`.
    ///
    /// See [`bevy::prelude::Events`] for details.
    ///
    /// If you wish to send this event to all clients, see [`Self::broadcast`].
    pub fn send(&mut self, event: E, client_id: ClientId) -> EventId<NettyEventToSend<E>> {
        self.ev_writer.send(NettyEventToSend {
            event,
            client_id: Some(client_id),
        })
    }

    /// Sends an `event`, which can later be read by [`EventReader`]s.
    /// This method returns the [ID](`EventId`) of the sent `event`.
    ///
    /// See [`bevy::prelude::Events`] for details.
    pub fn broadcast(&mut self, event: E) -> EventId<NettyEventToSend<E>> {
        self.ev_writer.send(NettyEventToSend { event, client_id: None })
    }

    /// Sends a list of `events` all at once, which can later be read by [`EventReader`]s.
    /// This is more efficient than sending each event individually.
    /// This method returns the [IDs](`EventId`) of the sent `events`.
    ///
    /// See [`bevy::prelude::Events`] for details.
    pub fn send_batch(&mut self, events: impl IntoIterator<Item = E>, client_id: Option<ClientId>) -> SendBatchIds<NettyEventToSend<E>> {
        self.ev_writer
            .send_batch(events.into_iter().map(|event| NettyEventToSend { event, client_id }))
    }

    /// Sends the default value of the event. Useful when the event is an empty struct.
    /// This method returns the [ID](`EventId`) of the sent `event`.
    ///
    /// See [`bevy::prelude::Events`] for details.
    pub fn send_default(&mut self, client_id: Option<ClientId>) -> EventId<NettyEventToSend<E>>
    where
        E: Default,
    {
        self.ev_writer.send(NettyEventToSend {
            event: E::default(),
            client_id,
        })
    }
}

fn receive_event(mut server: ResMut<RenetServer>, mut evw_got_event: EventWriter<GotNetworkEvent>) {
    for client_id in server.clients_id().into_iter() {
        while let Some(message) = server.receive_message(client_id, NettyChannelClient::NettyEvent) {
            let msg: NettyEventMessage = cosmos_encoder::deserialize(&message).unwrap_or_else(|e| {
                panic!("Failed to parse component replication message from client ({client_id})!\nError: {e:?}");
            });

            match msg {
                NettyEventMessage::SendNettyEvent { component_id, raw_data } => {
                    evw_got_event.send(GotNetworkEvent {
                        component_id,
                        raw_data,
                        client_id,
                    });
                }
            }
        }
    }
}

fn parse_event<T: NettyEvent>(
    events_registry: Res<Registry<RegisteredNettyEvent>>,
    mut evw_custom_event: EventWriter<NettyEventReceived<T>>,
    mut evr_need_parsed: EventReader<GotNetworkEvent>,
) {
    let Some(registered_event) = events_registry.from_id(T::unlocalized_name()) else {
        return;
    };

    for ev in evr_need_parsed.read() {
        if ev.component_id != registered_event.id() {
            continue;
        }

        let Ok(event) = bincode::deserialize::<T>(&ev.raw_data) else {
            error!("Got invalid event from client!");
            continue;
        };

        evw_custom_event.send(NettyEventReceived {
            event,
            client_id: ev.client_id,
        });
    }
}

fn send_events<T: NettyEvent>(
    mut server: ResMut<RenetServer>,
    mut evr: EventReader<NettyEventToSend<T>>,
    netty_event_registry: Res<Registry<RegisteredNettyEvent>>,
) {
    for ev in evr.read() {
        let Some(registered_event) = netty_event_registry.from_id(T::unlocalized_name()) else {
            error!("Event {} not regstered!\n{:?}", T::unlocalized_name(), netty_event_registry);
            continue;
        };

        let serialized = bincode::serialize(&ev.event).unwrap();

        if let Some(client_id) = ev.client_id {
            server.send_message(
                client_id,
                NettyChannelClient::NettyEvent,
                cosmos_encoder::serialize(&NettyEventMessage::SendNettyEvent {
                    component_id: registered_event.id(),
                    raw_data: serialized,
                }),
            );
        } else {
            server.broadcast_message(
                NettyChannelClient::NettyEvent,
                cosmos_encoder::serialize(&NettyEventMessage::SendNettyEvent {
                    component_id: registered_event.id(),
                    raw_data: serialized,
                }),
            );
        }
    }
}

fn server_receive_event<T: NettyEvent>(app: &mut App) {
    app.add_systems(Update, parse_event::<T>.in_set(NetworkingSystemsSet::ReceiveMessages))
        .add_event::<NettyEventReceived<T>>();
}

fn server_send_event<T: NettyEvent>(app: &mut App) {
    app.add_systems(Update, send_events::<T>.in_set(NetworkingSystemsSet::SyncComponents))
        .add_event::<NettyEventToSend<T>>();
}

fn register_event_type_impl<T: NettyEvent>(mut registry: ResMut<Registry<RegisteredNettyEvent>>) {
    registry.register(RegisteredNettyEvent {
        id: 0,
        unlocalized_name: T::unlocalized_name().into(),
    });
}

fn register_event_type<T: NettyEvent>(app: &mut App) {
    app.add_systems(OnEnter(GameState::Loading), register_event_type_impl::<T>);
}

pub(super) fn register_event<T: NettyEvent>(app: &mut App) {
    register_event_type::<T>(app);

    if T::event_receiver() == EventReceiver::Server || T::event_receiver() == EventReceiver::Both {
        server_receive_event::<T>(app);
    }
    if T::event_receiver() == EventReceiver::Client || T::event_receiver() == EventReceiver::Both {
        server_send_event::<T>(app);
    }
}

pub(super) fn register(app: &mut App) {
    app.add_systems(
        Update,
        receive_event
            .run_if(resource_exists::<RenetServer>)
            .in_set(NetworkingSystemsSet::ReceiveMessages),
    )
    .add_event::<GotNetworkEvent>();
}