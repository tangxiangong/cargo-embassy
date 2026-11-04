#![no_std]
#![no_main]
{{imports}}
use embassy_executor::Spawner;
use nrf_softdevice::{Softdevice, raw, ble::central};

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        ..Default::default()
    };
    let sd = Softdevice::enable(&config);
{{init}}
    spawner.spawn(softdevice_task(sd).unwrap());
    loop {
        let config = central::ScanConfig::default();
        let _ = central::scan(sd, &config, |_| {
            info!("BLE advertisement received");
            None::<()>
        }).await;
    }
}
