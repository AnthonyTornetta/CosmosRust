pub mod client_reliable_messages;
pub mod client_unreliable_messages;
pub mod netty_rigidbody;
pub mod server_laser_cannon_system_messages;
pub mod server_reliable_messages;
pub mod server_unreliable_messages;

use bevy::utils::default;
use bevy_renet::renet::{
    ChannelConfig, ReliableChannelConfig, RenetConnectionConfig, UnreliableChannelConfig,
};
use local_ip_address::local_ip;
use std::time::Duration;

pub enum NettyChannel {
    Reliable,
    Unreliable,
    LaserCannonSystem,
}

pub const PROTOCOL_ID: u64 = 7;

impl NettyChannel {
    pub fn id(&self) -> u8 {
        match self {
            Self::Reliable => 0,
            Self::Unreliable => 1,
            Self::LaserCannonSystem => 2,
        }
    }

    pub fn client_channels_config() -> Vec<ChannelConfig> {
        vec![
            ReliableChannelConfig {
                channel_id: Self::Reliable.id(),
                message_resend_time: Duration::from_millis(200),
                message_send_queue_size: 4096 * 4,
                message_receive_queue_size: 4096 * 4,
                max_message_size: 6000,
                packet_budget: 7000,
                ..default()
            }
            .into(),
            UnreliableChannelConfig {
                channel_id: Self::Unreliable.id(),
                message_send_queue_size: 4096 * 16,
                message_receive_queue_size: 4096 * 16,
                ..default()
            }
            .into(),
            UnreliableChannelConfig {
                channel_id: Self::LaserCannonSystem.id(),
                packet_budget: 7000,
                max_message_size: 6000,
                message_send_queue_size: 0,
                message_receive_queue_size: 4096 * 16,
                ..default()
            }
            .into(),
        ]
    }

    pub fn server_channels_config() -> Vec<ChannelConfig> {
        vec![
            ReliableChannelConfig {
                channel_id: Self::Reliable.id(),
                message_resend_time: Duration::from_millis(200),
                message_send_queue_size: 4096 * 4,
                message_receive_queue_size: 4096 * 4,
                max_message_size: 6000,
                packet_budget: 7000,
                ..default()
            }
            .into(),
            UnreliableChannelConfig {
                channel_id: Self::Unreliable.id(),
                message_send_queue_size: 4096 * 16,
                message_receive_queue_size: 4096 * 16,
                ..default()
            }
            .into(),
            UnreliableChannelConfig {
                channel_id: Self::LaserCannonSystem.id(),
                packet_budget: 7000,
                max_message_size: 6000,
                message_send_queue_size: 4096 * 16,
                message_receive_queue_size: 0,
                ..default()
            }
            .into(),
        ]
    }
}

pub fn client_connection_config() -> RenetConnectionConfig {
    RenetConnectionConfig {
        send_channels_config: NettyChannel::client_channels_config(),
        receive_channels_config: NettyChannel::client_channels_config(),
        ..default()
    }
}

pub fn server_connection_config() -> RenetConnectionConfig {
    RenetConnectionConfig {
        send_channels_config: NettyChannel::server_channels_config(),
        receive_channels_config: NettyChannel::server_channels_config(),
        ..default()
    }
}

pub fn get_local_ipaddress() -> String {
    if let Ok(ip) = local_ip() {
        ip.to_string()
    } else {
        "127.0.0.1".to_owned()
    }
}
