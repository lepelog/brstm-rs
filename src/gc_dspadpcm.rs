// translated from https://github.com/jackoalan/gc-dspadpcm-encode/blob/039712baa1291fbd77a1390e0496757122efd81b/grok.c

pub const PACKET_SAMPLES: usize = 14;
pub const PACKET_BYTES: usize = 8;

// need to start the buffer 2 bytes earlier to not
// read OoB
fn inner_product_merge(pcm_buf: &[i16; 16]) -> [f64; 3] {
    let mut out = [0.0f64; 3];
    for i in 0..=2 {
        out[i] = 0.0f64;
        for x in 0..14 {
            out[i] -= pcm_buf[x + 2 - i] as f64 * pcm_buf[x + 2] as f64;
        }
    }
    out
}

// need to start the buffer 2 bytes earlier to not
// read OoB
fn outer_product_merge(pcm_buf: &[i16; 16]) -> [[f64; 3]; 3] {
    let mut mtx_out = [[0.0f64; 3]; 3];
    for x in 1..=2 {
        for y in 1..=2 {
            mtx_out[x][y] = 0.0f64;
            for z in 0..14 {
                mtx_out[x][y] += pcm_buf[z + 2 - x] as f64 * pcm_buf[z + 2 - y] as f64;
            }
        }
    }
    mtx_out
}

fn analyze_ranges(mtx: &mut [[f64; 3]; 3], vec_idx_out: &mut [usize; 3]) -> bool {
    let mut recips = [0.0f64; 3];

    // get greatest distance from zero
    for x in 1..=2 {
        let val = mtx[x][1].abs().max(mtx[x][2].abs());
        if val < f64::EPSILON {
            return true;
        }
        recips[x] = 1.0f64 / val;
    }

    let mut max_index = 0;
    for i in 1..=2 {
        for x in 1..i {
            let mut tmp = mtx[x][i];
            for y in 1..x {
                tmp -= mtx[x][y] * mtx[y][i];
            }
            mtx[x][i] = tmp;
        }

        let mut val = 0.0f64;
        for x in i..=2 {
            let mut tmp = mtx[x][i];
            for y in 1..i {
                tmp -= mtx[x][y] * mtx[y][i];
            }
            mtx[x][i] = tmp;
            tmp = tmp.abs() * recips[x];
            if tmp >= val {
                val = tmp;
                max_index = x;
            }
        }

        if max_index != i {
            for y in 1..=2 {
                let tmp = mtx[max_index][y];
                mtx[max_index][y] = mtx[i][y];
                mtx[i][y] = tmp;
            }
            recips[max_index] = recips[i];
        }

        vec_idx_out[i] = max_index;

        if mtx[i][i] == 0.0f64 {
            return true;
        }

        if i != 2 {
            let tmp = 1.0 / mtx[i][i];
            for x in (i + 1)..=2 {
                mtx[x][i] *= tmp;
            }
        }
    }

    // get range
    let mut min = 10_000_000_000f64;
    let mut max = 0f64;
    for i in 1..=2 {
        let tmp = mtx[i][i].abs();
        if tmp < min {
            min = tmp;
        }
        if tmp > max {
            max = tmp;
        }
    }

    if min / max < 1.0e-10 {
        return true;
    }
    false
}

fn bidirectional_filter(mtx: &mut [[f64; 3]; 3], vec_idx: &[usize; 3], vec_out: &mut [f64; 3]) {
    let mut x = 0;
    for i in 1..=2 {
        let index = vec_idx[i];
        let mut tmp = vec_out[index];
        vec_out[index] = vec_out[i];
        if x != 0 {
            for y in x..=(i - 1) {
                tmp -= vec_out[y] * mtx[i][y];
            }
        } else if tmp != 0f64 {
            x = i;
        }
        vec_out[i] = tmp;
    }
    for i in (1..=2).rev() {
        let mut tmp = vec_out[i];
        for y in (i + 1)..=2 {
            tmp -= vec_out[y] * mtx[i][y];
        }
        vec_out[i] = tmp / mtx[i][i];
    }
    vec_out[0] = 0f64;
}

fn quadratic_merge(in_out_vec: &mut [f64; 3]) -> bool {
    let v2 = in_out_vec[2];
    let tmp = 1f64 - (v2 * v2);

    if tmp == 0f64 {
        return true;
    }

    let v0 = (in_out_vec[0] - (v2 * v2)) / tmp;
    let v1 = (in_out_vec[1] - (in_out_vec[1] * v2)) / tmp;

    in_out_vec[0] = v0;
    in_out_vec[1] = v1;

    v1.abs() > 1.0
}

fn finish_record(in_vec: &mut [f64; 3], out_vec: &mut [f64; 3]) {
    for v in in_vec.iter_mut().skip(1) {
        if *v >= 1f64 {
            *v = 0.9999999999f64;
        } else if *v <= -1f64 {
            *v = -0.9999999999f64;
        }
    }
    // for z in 1..=2 {
    //     if in_vec[z] >= 1f64 {
    //         in_vec[z] = 0.9999999999f64;
    //     } else if in_vec[z] <= -1f64 {
    //         in_vec[z] = -0.9999999999f64;
    //     }
    // }
    out_vec[0] = 1f64;
    out_vec[1] = (in_vec[2] * in_vec[1]) + in_vec[1];
    out_vec[2] = in_vec[2];
}

fn matrix_filter(src: &[f64; 3], dst: &mut [f64; 3]) {
    let mut mtx = [[0f64; 3]; 3];

    mtx[2][0] = 1f64;
    for i in 1..=2 {
        mtx[2][i] = -src[i];
    }

    for i in (1..=2).rev() {
        let val = 1f64 - (mtx[i][i] * mtx[i][i]);
        for y in 1..=i {
            mtx[i - 1][y] = ((mtx[i][i] * mtx[i][y]) + mtx[i][y]) / val;
        }
    }

    dst[0] = 1.0;
    for i in 1..=2 {
        dst[i] = 0.0;
        for y in 1..=i {
            dst[i] += mtx[i][y] * dst[i - y];
        }
    }
}

fn merge_finish_record(src: &mut [f64; 3], dst: &mut [f64; 3]) {
    let mut tmp = [0f64; 3];
    let mut val = src[0];

    dst[0] = 1.0;
    for i in 1..=2 {
        let mut v2 = 0f64;
        for y in 1..i {
            v2 += dst[y] * src[i - y];
        }

        if val > 0.0 {
            dst[i] = -(v2 + src[i]) / val;
        } else {
            dst[i] = 0.0;
        }

        tmp[i] = dst[i];

        for y in 1..i {
            dst[y] += dst[i] * dst[i - y];
        }

        val *= 1.0 - (dst[i] * dst[i]);
    }

    finish_record(&mut tmp, dst);
}

fn contrast_vectors(source1: &[f64; 3], source2: &[f64; 3]) -> f64 {
    let val = (source2[2] * source2[1] + -source2[1]) / (1.0 - source2[2] * source2[2]);
    let val1 = (source1[0] * source1[0]) + (source1[1] * source1[1]) + (source1[2] * source1[2]);
    let val2 = (source1[0] * source1[1]) + (source1[1] * source1[2]);
    let val3 = source1[0] * source1[2];
    val1 + (2.0 * val * val2) + (2.0 * (-source2[1] * val + -source2[2]) * val3)
}

fn filter_records(vec_best: &mut [[f64; 3]; 8], exp: usize, records: &[[f64; 3]]) {
    let mut buffer_list = [[0f64; 3]; 8];

    let mut buffer1 = [0isize; 8];
    let mut buffer2 = [0f64; 3];

    // let mut index = 0;
    // let mut value = 0f64;

    for _ in 0..2 {
        buffer1.fill(0);
        for buf in buffer_list.iter_mut() {
            buf.fill(0.0f64);
        }
        // for y in 0..exp {
        //     buffer1[y] = 0;
        //     for i in 0..=2 {
        //         buffer_list[y][i] = 0.0;
        //     }
        // }
        for record in records {
            let mut index = 0;
            let mut value = 1.0e30;
            for i in 0..exp {
                let temp_val = contrast_vectors(&vec_best[i], record);
                if temp_val < value {
                    value = temp_val;
                    index = i;
                }
            }
            buffer1[index] += 1;
            matrix_filter(record, &mut buffer2);
            for (buf, buf2) in buffer_list[index].iter_mut().zip(&buffer2) {
                *buf += buf2;
            }
            // for i in 0..=2 {
            //     buffer_list[index][i] += buffer2[i];
            // }
        }

        for (buf_list, buf1) in buffer_list.iter_mut().zip(&buffer1) {
            if *buf1 > 0 {
                for elem in buf_list {
                    *elem /= *buf1 as f64;
                }
            }
        }
        // for i in 0..exp {
        //     if buffer1[i] > 0 {
        //         for y in 0..=2 {
        //             buffer_list[i][y] /= buffer1[i] as f64;
        //         }
        //     }
        // }

        for (buf_list, vec) in buffer_list.iter_mut().zip(vec_best.iter_mut()) {
            merge_finish_record(buf_list, vec);
        }
        // for i in 0..exp {
        //     merge_finish_record(&mut buffer_list[i], &mut vec_best[i]);
        // }
    }
}

pub fn dsp_correlate_coefs(mut source: &[i16]) -> [[i16; 2]; 8] {
    let num_frames = source.len().div_ceil(14);
    let mut frame_samples;

    // let mut block_buffer = vec![0i16; 0x3800];
    let mut pcm_hist_buffer = [0i16; 2 * 14];

    let mut vec1 = [0f64; 3];
    let mut vec_idxs = [0; 3];

    let mut records: Vec<[f64; 3]> = Vec::with_capacity(num_frames * 2);
    let mut vec_best = [[0f64; 3]; 8];

    // iterate through 1024-block frames
    let mut x = source.len();
    while x > 0 {
        // full 1024-block frame
        if x > 0x3800 {
            frame_samples = 0x3800;
            x -= 0x3800;
        } else {
            // partial frame
            frame_samples = x;
            x = 0;
        }

        // block buffer copy, shouldn't be needed
        let block_buffer = source;
        source = &source[frame_samples..];

        let mut i = 0;
        while i < frame_samples {
            for z in 0..14 {
                pcm_hist_buffer[z] = pcm_hist_buffer[z + 14];
            }
            for z in 0..14 {
                pcm_hist_buffer[z + 14] = block_buffer.get(i).copied().unwrap_or(0);
                i += 1;
            }

            // usually this would be a buffer of 2 14 sample arrays, but inner_product_merge would read 2 elements
            // into the previous buffer in that case
            let fixed_hist_buffer: [i16; 16] = pcm_hist_buffer[12..][..16].try_into().unwrap();

            // 14 - 2, to prevent OoB reads
            let mut vec1 = inner_product_merge(&fixed_hist_buffer);
            if vec1[0].abs() > 10f64 {
                let mut mtx = outer_product_merge(&fixed_hist_buffer);
                if !analyze_ranges(&mut mtx, &mut vec_idxs) {
                    bidirectional_filter(&mut mtx, &vec_idxs, &mut vec1);
                    if !quadratic_merge(&mut vec1) {
                        let mut out_vec = [0f64; 3];
                        finish_record(&mut vec1, &mut out_vec);
                        records.push(out_vec);
                    }
                }
            }
        }
    }

    vec1[0] = 1f64;
    vec1[1] = 0f64;
    vec1[2] = 0f64;

    for record in &records {
        matrix_filter(record, &mut vec_best[0]);
        for y in 1..=2 {
            vec1[y] += vec_best[0][y];
        }
    }
    for y in 1..=2 {
        vec1[y] /= records.len() as f64;
    }
    merge_finish_record(&mut vec1, &mut vec_best[0]);
    let mut exp = 1;
    for w in 0..3 {
        let vec2 = [0f64, -1f64, 0f64];
        for i in 0..exp {
            for y in 0..=2 {
                vec_best[exp + i][y] = (0.01 * vec2[y]) + vec_best[i][y];
            }
        }
        exp = 1 << (w + 1);
        filter_records(&mut vec_best, exp, &records);
    }

    let mut coefs_out = [[0; 2]; 8];

    for z in 0..8 {
        let d = -vec_best[z][1] * 2048f64;
        if d > 0f64 {
            coefs_out[z][0] = if d > 32767f64 {
                32767
            } else {
                d.round() as i16
            };
        } else {
            coefs_out[z][0] = if d < -32768f64 {
                -32768
            } else {
                d.round() as i16
            };
        }
        let d = -vec_best[z][2] * 2048f64;
        if d > 0f64 {
            coefs_out[z][1] = if d > 32767f64 {
                32767
            } else {
                d.round() as i16
            };
        } else {
            coefs_out[z][1] = if d < -32768f64 {
                -32768
            } else {
                d.round() as i16
            };
        }
    }

    coefs_out
}

pub fn dsp_encode_frame(
    pcm_in_out: &mut [i16; 16],
    sample_count: usize,
    coefs_in: &[[i16; 2]; 8],
) -> [u8; 8] {
    let mut in_samples = [[0; 16]; 8];
    let mut out_samples = [[0; 14]; 8];

    let mut best_index = 0;

    let mut scale = [0isize; 8];
    let mut dist_accum = [0f64; 8];

    /* Iterate through each coef set, finding the set with the smallest error */
    for i in 0..8 {
        // int v1, v2, v3;
        // int distance, index;

        /* Set yn values */
        in_samples[i][0] = pcm_in_out[0];
        in_samples[i][1] = pcm_in_out[1];

        /* Round and clamp samples for this coef set */
        let mut distance: isize = 0;
        for s in 0..sample_count {
            /* Multiply previous samples by coefs */
            let v1 = ((pcm_in_out[s] as isize * coefs_in[i][1] as isize)
                + (pcm_in_out[s + 1] as isize * coefs_in[i][0] as isize))
                / 2048;
            in_samples[i][s + 2] = v1 as i16;
            /* Subtract from current sample */
            let v2 = pcm_in_out[s + 2] as isize - v1;
            /* Clamp */
            let v3 = v2.clamp(i16::MIN.into(), i16::MAX.into());
            /* Compare distance */
            if v3.abs() > distance.abs() {
                distance = v3;
            }
        }

        /* Set initial scale */
        scale[i] = 0;
        while (scale[i] <= 12) && !(-8..=7).contains(&distance) {
            scale[i] += 1;
            distance /= 2;
        }
        scale[i] = if scale[i] <= 1 { -1 } else { scale[i] - 2 };

        loop {
            scale[i] += 1;
            dist_accum[i] = 0.0;
            let mut index = 0;

            for s in 0..sample_count {
                /* Multiply previous */
                let mut v1 = (in_samples[i][s] as isize * coefs_in[i][1] as isize)
                    + (in_samples[i][s + 1] as isize * coefs_in[i][0] as isize);
                /* Evaluate from real sample */
                let mut v2 = (((pcm_in_out[s + 2] as isize) << 11) - v1) / 2048;
                /* Round to nearest sample */
                let mut v3 = if v2 > 0 {
                    (v2 as f64 / (1 << scale[i]) as f64 + 0.4999999f64) as isize
                } else {
                    (v2 as f64 / (1 << scale[i]) as f64 - 0.4999999f64) as isize
                };

                /* Clamp sample and set index */
                if v3 < -8 {
                    v3 = -8 - v3;
                    if index < v3 {
                        index = v3;
                    }
                    v3 = -8;
                } else if v3 > 7 {
                    v3 -= 7;
                    if index < v3 {
                        index = v3;
                    }
                    v3 = 7;
                }

                /* Store result */
                out_samples[i][s] = v3;

                /* Round and expand */
                v1 = (v1 + ((v3 * (1 << scale[i])) << 11) + 1024) >> 11;
                /* Clamp and store */
                v2 = v1.clamp(i16::MIN.into(), i16::MAX.into());
                in_samples[i][s + 2] = v2 as i16;
                /* Accumulate distance */
                v3 = pcm_in_out[s + 2] as isize - v2;
                dist_accum[i] += v3 as f64 * v3 as f64;
            }

            let mut x = index + 8;
            while x > 256 {
                scale[i] += 1;
                if scale[i] >= 12 {
                    scale[i] = 11;
                }
                x >>= 1;
            }

            if !((scale[i] < 12) && (index > 1)) {
                break;
            }
        }
    }

    let mut min = f64::MAX;
    for i in 0..8 {
        if dist_accum[i] < min {
            min = dist_accum[i];
            best_index = i;
        }
    }

    /* Write converted samples */
    pcm_in_out[2..(sample_count + 2)]
        .copy_from_slice(&in_samples[best_index][2..(sample_count + 2)]);

    let mut adpcm_out = [0; 8];

    /* Write ps */
    adpcm_out[0] = ((best_index << 4) | (scale[best_index] as usize & 0xF)) as u8;

    /* Zero remaining samples */
    for s in sample_count..14 {
        out_samples[best_index][s] = 0;
    }

    /* Write output samples */
    for y in 0..7 {
        adpcm_out[y + 1] = ((out_samples[best_index][y * 2] << 4)
            | (out_samples[best_index][y * 2 + 1] & 0xF)) as u8;
    }

    adpcm_out
}
