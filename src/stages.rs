use lazy_static::lazy_static;

use std::collections::HashMap;

lazy_static! {
    static ref ALIAS2STAGE: HashMap<&'static str, &'static str> = HashMap::from([
        ("cap", "CapWorldHomeStage"),
        ("cascade", "WaterfallWorldHomeStage"),
        ("sand", "SandWorldHomeStage"),
        ("lake", "LakeWorldHomeStage"),
        ("wooded", "ForestWorldHomeStage"),
        ("cloud", "CloudWorldHomeStage"),
        ("lost", "ClashWorldHomeStage"),
        ("metro", "CityWorldHomeStage"),
        ("snow", "SnowWorldHomeStage"),
        ("sea", "SeaWorldHomeStage"),
        ("lunch", "LavaWorldHomeStage"),
        ("ruined", "BossRaidWorldHomeStage"),
        ("bowser", "SkyWorldHomeStage"),
        ("moon", "MoonWorldHomeStage"),
        ("mush", "PeachWorldHomeStage"),
        ("dark", "Special1WorldHomeStage"),
        ("darker", "Special2WorldHomeStage"),
        ("odyssey", "HomeShipInsideStage"),
    ]);
    static ref ALIAS2KINGDOM: HashMap<&'static str, &'static str> = HashMap::from([
        ("cap", "Cap Kingdom"),
        ("cascade", "Cascade Kingdom"),
        ("sand", "Sand Kingdom"),
        ("lake", "Lake Kingdom"),
        ("wooded", "Wooded Kingdom"),
        ("cloud", "Cloud Kingdom"),
        ("lost", "Lost Kingdom"),
        ("metro", "Metro Kingdom"),
        ("snow", "Snow Kingdom"),
        ("sea", "Seaside Kingdom"),
        ("lunch", "Luncheon Kingdom"),
        ("ruined", "Ruined Kingdom"),
        ("bowser", "Bowser's Kingdom"),
        ("moon", "Moon Kingdom"),
        ("mush", "Mushroom Kingdom"),
        ("dark", "Dark Side"),
        ("darker", "Darker Side"),
        ("odyssey", "Odyssey"),
    ]);
    static ref STAGE2ALIAS: HashMap<&'static str, &'static str> = HashMap::from([
        ("CapWorldHomeStage", "cap"),
        ("CapWorldTowerStage", "cap"),
        ("FrogSearchExStage", "cap"),
        ("PoisonWaveExStage", "cap"),
        ("PushBlockExStage", "cap"),
        ("RollingExStage", "cap"),
        ("WaterfallWorldHomeStage", "cascade"),
        ("TrexPoppunExStage", "cascade"),
        ("Lift2DExStage", "cascade"),
        ("WanwanClashExStage", "cascade"),
        ("CapAppearExStage", "cascade"),
        ("WindBlowExStage", "cascade"),
        ("SandWorldHomeStage", "sand"),
        ("SandWorldShopStage", "sand"),
        ("SandWorldSlotStage", "sand"),
        ("SandWorldVibrationStage", "sand"),
        ("SandWorldSecretStage", "sand"),
        ("SandWorldMeganeExStage", "sand"),
        ("SandWorldKillerExStage", "sand"),
        ("SandWorldPressExStage", "sand"),
        ("SandWorldSphinxExStage", "sand"),
        ("SandWorldCostumeStage", "sand"),
        ("SandWorldPyramid000Stage", "sand"),
        ("SandWorldPyramid001Stage", "sand"),
        ("SandWorldUnderground000Stage", "sand"),
        ("SandWorldUnderground001Stage", "sand"),
        ("SandWorldRotateExStage", "sand"),
        ("MeganeLiftExStage", "sand"),
        ("RocketFlowerExStage", "sand"),
        ("WaterTubeExStage", "sand"),
        ("LakeWorldHomeStage", "lake"),
        ("LakeWorldShopStage", "lake"),
        ("FastenerExStage", "lake"),
        ("TrampolineWallCatchExStage", "lake"),
        ("GotogotonExStage", "lake"),
        ("FrogPoisonExStage", "lake"),
        ("ForestWorldHomeStage", "wooded"),
        ("ForestWorldWaterExStage", "wooded"),
        ("ForestWorldTowerStage", "wooded"),
        ("ForestWorldBossStage", "wooded"),
        ("ForestWorldBonusStage", "wooded"),
        ("ForestWorldCloudBonusExStage", "wooded"),
        ("FogMountainExStage", "wooded"),
        ("RailCollisionExStage", "wooded"),
        ("ShootingElevatorExStage", "wooded"),
        ("ForestWorldWoodsStage", "wooded"),
        ("ForestWorldWoodsTreasureStage", "wooded"),
        ("ForestWorldWoodsCostumeStage", "wooded"),
        ("PackunPoisonExStage", "wooded"),
        ("AnimalChaseExStage", "wooded"),
        ("KillerRoadExStage", "wooded"),
        ("CloudWorldHomeStage", "cloud"),
        ("FukuwaraiKuriboStage", "cloud"),
        ("Cube2DExStage", "cloud"),
        ("ClashWorldHomeStage", "lost"),
        ("ClashWorldShopStage", "lost"),
        ("ImomuPoisonExStage", "lost"),
        ("JangoExStage", "lost"),
        ("CityWorldHomeStage", "metro"),
        ("CityWorldMainTowerStage", "metro"),
        ("CityWorldFactoryStage", "metro"),
        ("CityWorldShop01Stage", "metro"),
        ("CityWorldSandSlotStage", "metro"),
        ("CityPeopleRoadStage", "metro"),
        ("PoleGrabCeilExStage", "metro"),
        ("TrexBikeExStage", "metro"),
        ("PoleKillerExStage", "metro"),
        ("Note2D3DRoomExStage", "metro"),
        ("ShootingCityExStage", "metro"),
        ("CapRotatePackunExStage", "metro"),
        ("RadioControlExStage", "metro"),
        ("ElectricWireExStage", "metro"),
        ("Theater2DExStage", "metro"),
        ("DonsukeExStage", "metro"),
        ("SwingSteelExStage", "metro"),
        ("BikeSteelExStage", "metro"),
        ("SnowWorldHomeStage", "snow"),
        ("SnowWorldTownStage", "snow"),
        ("SnowWorldShopStage", "snow"),
        ("SnowWorldLobby000Stage", "snow"),
        ("SnowWorldLobby001Stage", "snow"),
        ("SnowWorldRaceTutorialStage", "snow"),
        ("SnowWorldRace000Stage", "snow"),
        ("SnowWorldRace001Stage", "snow"),
        ("SnowWorldCostumeStage", "snow"),
        ("SnowWorldCloudBonusExStage", "snow"),
        ("IceWalkerExStage", "snow"),
        ("IceWaterBlockExStage", "snow"),
        ("ByugoPuzzleExStage", "snow"),
        ("IceWaterDashExStage", "snow"),
        ("SnowWorldLobbyExStage", "snow"),
        ("SnowWorldRaceExStage", "snow"),
        ("SnowWorldRaceHardExStage", "snow"),
        ("KillerRailCollisionExStage", "snow"),
        ("SeaWorldHomeStage", "sea"),
        ("SeaWorldUtsuboCaveStage", "sea"),
        ("SeaWorldVibrationStage", "sea"),
        ("SeaWorldSecretStage", "sea"),
        ("SeaWorldCostumeStage", "sea"),
        ("SeaWorldSneakingManStage", "sea"),
        ("SenobiTowerExStage", "sea"),
        ("CloudExStage", "sea"),
        ("WaterValleyExStage", "sea"),
        ("ReflectBombExStage", "sea"),
        ("TogezoRotateExStage", "sea"),
        ("LavaWorldHomeStage", "lunch"),
        ("LavaWorldUpDownExStage", "lunch"),
        ("LavaBonus1Zone", "lunch"),
        ("LavaWorldShopStage", "lunch"),
        ("LavaWorldCostumeStage", "lunch"),
        ("ForkExStage", "lunch"),
        ("LavaWorldExcavationExStage", "lunch"),
        ("LavaWorldClockExStage", "lunch"),
        ("LavaWorldBubbleLaneExStage", "lunch"),
        ("LavaWorldTreasureStage", "lunch"),
        ("GabuzouClockExStage", "lunch"),
        ("CapAppearLavaLiftExStage", "lunch"),
        ("LavaWorldFenceLiftExStage", "lunch"),
        ("BossRaidWorldHomeStage", "ruined"),
        ("DotTowerExStage", "ruined"),
        ("BullRunExStage", "ruined"),
        ("SkyWorldHomeStage", "bowser"),
        ("SkyWorldShopStage", "bowser"),
        ("SkyWorldCostumeStage", "bowser"),
        ("SkyWorldCloudBonusExStage", "bowser"),
        ("SkyWorldTreasureStage", "bowser"),
        ("JizoSwitchExStage", "bowser"),
        ("TsukkunRotateExStage", "bowser"),
        ("KaronWingTowerStage", "bowser"),
        ("TsukkunClimbExStage", "bowser"),
        ("MoonWorldHomeStage", "moon"),
        ("MoonWorldCaptureParadeStage", "moon"),
        ("MoonWorldWeddingRoomStage", "moon"),
        ("MoonWorldKoopa1Stage", "moon"),
        ("MoonWorldBasementStage", "moon"),
        ("MoonWorldWeddingRoom2Stage", "moon"),
        ("MoonWorldKoopa2Stage", "moon"),
        ("MoonWorldShopRoom", "moon"),
        ("MoonWorldSphinxRoom", "moon"),
        ("MoonAthleticExStage", "moon"),
        ("Galaxy2DExStage", "moon"),
        ("PeachWorldHomeStage", "mush"),
        ("PeachWorldShopStage", "mush"),
        ("PeachWorldCastleStage", "mush"),
        ("PeachWorldCostumeStage", "mush"),
        ("FukuwaraiMarioStage", "mush"),
        ("DotHardExStage", "mush"),
        ("YoshiCloudExStage", "mush"),
        ("PeachWorldPictureBossMagmaStage", "mush"),
        ("RevengeBossMagmaStage", "mush"),
        ("PeachWorldPictureGiantWanderBossStage", "mush"),
        ("RevengeGiantWanderBossStage", "mush"),
        ("PeachWorldPictureBossKnuckleStage", "mush"),
        ("RevengeBossKnuckleStage", "mush"),
        ("PeachWorldPictureBossForestStage", "mush"),
        ("RevengeForestBossStage", "mush"),
        ("PeachWorldPictureMofumofuStage", "mush"),
        ("RevengeMofumofuStage", "mush"),
        ("PeachWorldPictureBossRaidStage", "mush"),
        ("RevengeBossRaidStage", "mush"),
        ("Special1WorldHomeStage", "dark"),
        ("Special1WorldTowerStackerStage", "dark"),
        ("Special1WorldTowerBombTailStage", "dark"),
        ("Special1WorldTowerFireBlowerStage", "dark"),
        ("Special1WorldTowerCapThrowerStage", "dark"),
        ("KillerRoadNoCapExStage", "dark"),
        ("PackunPoisonNoCapExStage", "dark"),
        ("BikeSteelNoCapExStage", "dark"),
        ("ShootingCityYoshiExStage", "dark"),
        ("SenobiTowerYoshiExStage", "dark"),
        ("LavaWorldUpDownYoshiExStage", "dark"),
        ("Special2WorldHomeStage", "darker"),
        ("Special2WorldLavaStage", "darker"),
        ("Special2WorldCloudStage", "darker"),
        ("Special2WorldKoopaStage", "darker"),
        ("HomeShipInsideStage", "odyssey"),
    ]);
}

pub struct Stages {}

impl Stages {
    pub fn input2stage(input: &str) -> Option<String> {
        // alias value
        if Self::is_alias(input) {
            return match ALIAS2STAGE.get(&input) {
                Some(stage) => Some(stage.to_string()),
                None => None,
            };
        }
        // exact stage value
        if Self::is_stage(input) {
            return Some(input.to_string());
        }
        // force input value with a !
        if input.ends_with("!") {
            return Some(input[0..(input.len() - 1)].to_string());
        }
        return None;
    }

    pub fn stage2kingdom(stage: &str) -> Option<String> {
        match STAGE2ALIAS.get(&stage) {
            Some(alias) => match ALIAS2KINGDOM.get(alias) {
                Some(kingdom) => Some(kingdom.to_string()),
                None => None,
            },
            None => None,
        }
    }

    pub fn is_alias(input: &str) -> bool {
        return ALIAS2STAGE.contains_key(&input);
    }

    pub fn is_stage(input: &str) -> bool {
        return STAGE2ALIAS.contains_key(&input);
    }

    pub fn stages_by_input(input: &str) -> Vec<String> {
        if Self::is_alias(input) {
            return STAGE2ALIAS.iter().filter(|(_k,v)| **v == input).map(|(k,_v)| k.to_string()).collect::<Vec<_>>();
        }

        return match Self::input2stage(input) {
            Some(stage) => [stage].to_vec(),
            _ => [].to_vec(),
        }
    }
}
