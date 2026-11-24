//! The joints of the mannequin and the left/right pairs a mirror swaps.

/// A humanoid's joints. `+X` is the character's right, `+Y` is up and `+Z` is forward.
///
/// Fingers and toes beyond a single joint are deliberately absent: a drawing figure is a gesture,
/// and a hand made of fifteen joints is a different product.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Joint {
    Pelvis,
    Spine1,
    Spine2,
    Chest,
    Neck,
    Head,
    ClavicleL,
    UpperArmL,
    ForearmL,
    HandL,
    ClavicleR,
    UpperArmR,
    ForearmR,
    HandR,
    ThighL,
    ShinL,
    FootL,
    ToeL,
    ThighR,
    ShinR,
    FootR,
    ToeR,
}

impl Joint {
    pub const COUNT: usize = 22;

    pub const ALL: [Joint; Self::COUNT] = [
        Joint::Pelvis,
        Joint::Spine1,
        Joint::Spine2,
        Joint::Chest,
        Joint::Neck,
        Joint::Head,
        Joint::ClavicleL,
        Joint::UpperArmL,
        Joint::ForearmL,
        Joint::HandL,
        Joint::ClavicleR,
        Joint::UpperArmR,
        Joint::ForearmR,
        Joint::HandR,
        Joint::ThighL,
        Joint::ShinL,
        Joint::FootL,
        Joint::ToeL,
        Joint::ThighR,
        Joint::ShinR,
        Joint::FootR,
        Joint::ToeR,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The joint on the other side of the body, or itself for the spine.
    pub fn mirror(self) -> Joint {
        use Joint::*;
        match self {
            ClavicleL => ClavicleR,
            ClavicleR => ClavicleL,
            UpperArmL => UpperArmR,
            UpperArmR => UpperArmL,
            ForearmL => ForearmR,
            ForearmR => ForearmL,
            HandL => HandR,
            HandR => HandL,
            ThighL => ThighR,
            ThighR => ThighL,
            ShinL => ShinR,
            ShinR => ShinL,
            FootL => FootR,
            FootR => FootL,
            ToeL => ToeR,
            ToeR => ToeL,
            other => other,
        }
    }

    pub fn name(self) -> &'static str {
        use Joint::*;
        match self {
            Pelvis => "pelvis",
            Spine1 => "spine1",
            Spine2 => "spine2",
            Chest => "chest",
            Neck => "neck",
            Head => "head",
            ClavicleL => "clavicle_l",
            UpperArmL => "upper_arm_l",
            ForearmL => "forearm_l",
            HandL => "hand_l",
            ClavicleR => "clavicle_r",
            UpperArmR => "upper_arm_r",
            ForearmR => "forearm_r",
            HandR => "hand_r",
            ThighL => "thigh_l",
            ShinL => "shin_l",
            FootL => "foot_l",
            ToeL => "toe_l",
            ThighR => "thigh_r",
            ShinR => "shin_r",
            FootR => "foot_r",
            ToeR => "toe_r",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_joint_has_a_unique_index() {
        let mut seen = [false; Joint::COUNT];
        for joint in Joint::ALL {
            let index = joint.index();
            assert!(index < Joint::COUNT);
            assert!(!seen[index], "two joints share index {index}");
            seen[index] = true;
        }
        assert!(seen.iter().all(|used| *used));
    }

    #[test]
    fn mirror_twice_is_the_original() {
        for joint in Joint::ALL {
            assert_eq!(joint.mirror().mirror(), joint);
        }
    }
}
