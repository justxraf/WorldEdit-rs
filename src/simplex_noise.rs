const F3: f64 = 1.0 / 3.0;
const G3: f64 = 1.0 / 6.0;

const GRAD3: [(f64, f64, f64); 12] = [
    (1.0, 1.0, 0.0),
    (-1.0, 1.0, 0.0),
    (1.0, -1.0, 0.0),
    (-1.0, -1.0, 0.0),
    (1.0, 0.0, 1.0),
    (-1.0, 0.0, 1.0),
    (1.0, 0.0, -1.0),
    (-1.0, 0.0, -1.0),
    (0.0, 1.0, 1.0),
    (0.0, -1.0, 1.0),
    (0.0, 1.0, -1.0),
    (0.0, -1.0, -1.0),
];

const PERM: [u8; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30, 69,
    142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219,
    203, 117, 35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175,
    74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, 60, 211, 133, 230,
    220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1, 216, 80, 73, 209, 76,
    132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173,
    186, 3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, 207, 206,
    59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213, 119, 248, 152, 2, 44, 154, 163,
    70, 221, 153, 101, 155, 167, 43, 172, 9, 129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232,
    178, 185, 112, 104, 218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162,
    241, 81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157, 184, 84, 204,
    176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141,
    128, 195, 78, 66, 215, 61, 156, 180,
];

pub fn noise3(x: f64, y: f64, z: f64) -> f64 {
    let skew = (x + y + z) * F3;
    let i = fast_floor(x + skew);
    let j = fast_floor(y + skew);
    let k = fast_floor(z + skew);

    let unskew = f64::from(i + j + k) * G3;
    let x0 = x - (f64::from(i) - unskew);
    let y0 = y - (f64::from(j) - unskew);
    let z0 = z - (f64::from(k) - unskew);

    let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
        if y0 >= z0 {
            (1, 0, 0, 1, 1, 0)
        } else if x0 >= z0 {
            (1, 0, 0, 1, 0, 1)
        } else {
            (0, 0, 1, 1, 0, 1)
        }
    } else if y0 < z0 {
        (0, 0, 1, 0, 1, 1)
    } else if x0 < z0 {
        (0, 1, 0, 0, 1, 1)
    } else {
        (0, 1, 0, 1, 1, 0)
    };

    let x1 = x0 - f64::from(i1) + G3;
    let y1 = y0 - f64::from(j1) + G3;
    let z1 = z0 - f64::from(k1) + G3;
    let x2 = x0 - f64::from(i2) + 2.0 * G3;
    let y2 = y0 - f64::from(j2) + 2.0 * G3;
    let z2 = z0 - f64::from(k2) + 2.0 * G3;
    let x3 = x0 - 1.0 + 3.0 * G3;
    let y3 = y0 - 1.0 + 3.0 * G3;
    let z3 = z0 - 1.0 + 3.0 * G3;

    let ii = (i & 255) as usize;
    let jj = (j & 255) as usize;
    let kk = (k & 255) as usize;

    let gi0 = perm(ii + perm(jj + perm(kk))) % 12;
    let gi1 = perm(ii + i1 as usize + perm(jj + j1 as usize + perm(kk + k1 as usize))) % 12;
    let gi2 = perm(ii + i2 as usize + perm(jj + j2 as usize + perm(kk + k2 as usize))) % 12;
    let gi3 = perm(ii + 1 + perm(jj + 1 + perm(kk + 1))) % 12;

    32.0 * (contribution(gi0, x0, y0, z0)
        + contribution(gi1, x1, y1, z1)
        + contribution(gi2, x2, y2, z2)
        + contribution(gi3, x3, y3, z3))
}

fn contribution(gradient_index: usize, x: f64, y: f64, z: f64) -> f64 {
    let t = 0.6 - x * x - y * y - z * z;
    if t < 0.0 {
        return 0.0;
    }
    let t2 = t * t;
    let (gx, gy, gz) = GRAD3[gradient_index];
    t2 * t2 * (gx * x + gy * y + gz * z)
}

fn fast_floor(value: f64) -> i32 {
    let truncated = value as i32;
    if value < f64::from(truncated) {
        truncated - 1
    } else {
        truncated
    }
}

fn perm(index: usize) -> usize {
    PERM[index & 255] as usize
}

#[cfg(test)]
mod tests {
    use super::noise3;

    fn approx_eq(left: f64, right: f64) {
        assert!((left - right).abs() < 1.0e-9, "{left} != {right}");
    }

    #[test]
    fn matches_fawe_probe_samples() {
        approx_eq(noise3(0.0, 0.0, 0.0), 0.0);
        approx_eq(noise3(0.5, 0.0, 0.0), 0.429_757_201_646_09);
        approx_eq(noise3(1.0, 0.0, 0.0), -0.760_099_588_477_365_6);
        approx_eq(noise3(0.3, 0.7, 1.1), -0.325_865_792);
    }
}
