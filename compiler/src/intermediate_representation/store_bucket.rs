use super::ir_interface::*;
use crate::translating_traits::*;
use code_producers::c_elements::*;
use code_producers::wasm_elements::*;

#[derive(Clone)]
pub struct StoreBucket {
    pub line: usize,
    pub message_id: usize,
    pub context: InstrContext,
    pub dest_is_output: bool,
    pub dest_address_type: AddressType,
    pub dest: LocationRule,
    pub src: InstructionPointer,
}

impl IntoInstruction for StoreBucket {
    fn into_instruction(self) -> Instruction {
        Instruction::Store(self)
    }
}

impl Allocate for StoreBucket {
    fn allocate(self) -> InstructionPointer {
        InstructionPointer::new(self.into_instruction())
    }
}

impl ObtainMeta for StoreBucket {
    fn get_line(&self) -> usize {
        self.line
    }
    fn get_message_id(&self) -> usize {
        self.message_id
    }
}

impl ToString for StoreBucket {
    fn to_string(&self) -> String {
        let line = self.line.to_string();
        let template_id = self.message_id.to_string();
        let dest_type = self.dest_address_type.to_string();
        let dest = self.dest.to_string();
        let src = self.src.to_string();
        format!(
            "STORE(line:{},template_id:{},dest_type:{},dest:{},src:{})",
            line, template_id, dest_type, dest, src
        )
    }
}

impl WriteWasm for StoreBucket {
    fn produce_wasm(&self, producer: &WASMProducer) -> Vec<String> {
        use code_producers::wasm_elements::wasm_code_generator::*;
        let mut instructions = vec![];
        if self.context.size == 0 {
            return vec![];
        }
        if producer.needs_comments() {
	    instructions.push(format!(";; store bucket. Line {}", self.line)); //.to_string()
	}
        let mut my_template_header = Option::<String>::None;
        if producer.needs_comments() {
            instructions.push(";; getting dest".to_string());
	}
        match &self.dest {
            LocationRule::Indexed { location, template_header } => {
                let mut instructions_dest = location.produce_wasm(producer);
                instructions.append(&mut instructions_dest);
                let size = producer.get_size_32_bits_in_memory() * 4;
                instructions.push(set_constant(&size.to_string()));
                instructions.push(mul32());
                match &self.dest_address_type {
                    AddressType::Variable => {
                        instructions.push(get_local(producer.get_lvar_tag()));
                    }
                    AddressType::Signal => {
                        instructions.push(get_local(producer.get_signal_start_tag()));
                    }
                    AddressType::SubcmpSignal { cmp_address, .. } => {
                        my_template_header = template_header.clone();
                        instructions.push(get_local(producer.get_offset_tag()));
                        instructions.push(set_constant(
                            &producer.get_sub_component_start_in_component().to_string(),
                        ));
                        instructions.push(add32());
                        let mut instructions_sci = cmp_address.produce_wasm(producer);
                        instructions.append(&mut instructions_sci);
                        instructions.push(set_constant("4")); //size in byte of i32
                        instructions.push(mul32());
                        instructions.push(add32());
                        instructions.push(load32(None)); //subcomponent block
                        instructions.push(set_local(producer.get_sub_cmp_tag()));
                        instructions.push(get_local(producer.get_sub_cmp_tag()));
                        instructions.push(set_constant(
                            &producer.get_signal_start_address_in_component().to_string(),
                        ));
                        instructions.push(add32());
                        instructions.push(load32(None)); //subcomponent start_of_signals
                    }
                }
                instructions.push(add32());
            }
            LocationRule::Mapped { signal_code, indexes } => {
                match &self.dest_address_type {
                    AddressType::SubcmpSignal { cmp_address, .. } => {
			if producer.needs_comments() {
                            instructions.push(";; is subcomponent mapped".to_string());
			}
                        instructions.push(get_local(producer.get_offset_tag()));
                        instructions.push(set_constant(
                            &producer.get_sub_component_start_in_component().to_string(),
                        ));
                        instructions.push(add32());
                        let mut instructions_sci = cmp_address.produce_wasm(producer);
                        instructions.append(&mut instructions_sci);
                        instructions.push(set_constant("4")); //size in byte of i32
                        instructions.push(mul32());
                        instructions.push(add32());
                        instructions.push(load32(None)); //subcomponent block
                        instructions.push(tee_local(producer.get_sub_cmp_tag()));
                        //instructions.push(set_local(producer.get_sub_cmp_tag()));
                        //instructions.push(get_local(producer.get_sub_cmp_tag()));
                        instructions.push(load32(None)); // get template id                     A
                        instructions.push(set_constant("4")); //size in byte of i32
                        instructions.push(mul32());
                        instructions.push(load32(Some(
                            &producer.get_template_instance_to_io_signal_start().to_string(),
                        ))); // get position in component io signal to info list
                        let signal_code_in_bytes = signal_code * 4; //position in the list of the signal code
                        instructions.push(load32(Some(&signal_code_in_bytes.to_string()))); // get where the info of this signal is
                        //now we have first the offset, and then the all size dimensions but the last one
			if indexes.len() == 0 {
			    //instructions.push(";; has no indexes".to_string());
			    instructions.push(load32(None)); // get signal offset (it is already the actual one in memory);
			} else {
			    //instructions.push(";; has indexes".to_string());
			    instructions.push(tee_local(producer.get_io_info_tag()));
			    instructions.push(load32(None)); // get offset; first slot in io_info (to start adding offsets)
			    // if the first access is qualified we place the address of the bus_id
			    if let AccessType::Qualified(_) = &indexes[0] {
				instructions.push(get_local(producer.get_io_info_tag()));
				instructions.push(load32(Some("4"))); // it is a bus, so the bus_id is in the second position
			    }
			    let mut idxpos = 0;			    
			    while idxpos < indexes.len() {
				if let AccessType::Indexed(index_list) = &indexes[idxpos] {
				    let mut infopos = 0;
				    assert!(index_list.len() > 0);
				    //We first compute the number of elements as
				    //((index_list[0] * length_of_dim[1]) + index_list[1]) * length_of_dim[2] + ... )* length_of_dim[n-1] + index_list[n-1]
				    //first position in the array access
				    let mut instructions_idx0 = index_list[0].produce_wasm(producer);				    
				    instructions.append(&mut instructions_idx0);				    
				    for i in 1..index_list.len() {
					instructions.push(get_local(producer.get_io_info_tag()));
					infopos += 4;	//position in io or bus info of dimension of [1] (recall that first dimension is not added)
					instructions.push(load32(Some(&infopos.to_string()))); // second dimension
					instructions.push(mul32());
					let mut instructions_idxi = index_list[i].produce_wasm(producer);				    
					instructions.append(&mut instructions_idxi);				    
					instructions.push(add32());
				    }
				    let field_size = producer.get_size_32_bits_in_memory() * 4;
				    instructions.push(set_constant(&field_size.to_string()));
				    instructions.push(get_local(producer.get_io_info_tag()));
				    infopos += 4; //position in io or bus info of size 
				    instructions.push(load32(Some(&infopos.to_string()))); // size
				    instructions.push(mul32()); // size mult by size of field in bytes
				    instructions.push(mul32()); // total offset in the array
				    instructions.push(add32()); // to the current offset
				    idxpos += 1;
				    if idxpos < indexes.len() {
					//next must be Qualified
					if let AccessType::Indexed(_) = &indexes[idxpos] {
					    assert!(false);
					}
					// we add the type of bus it is
					instructions.push(get_local(producer.get_io_info_tag()));
					infopos += 4;
					instructions.push(load32(Some(&infopos.to_string()))); // bus_id
				    }
				} else if let AccessType::Qualified(field_no) = &indexes[idxpos] {
				    //we have on the stack the bus_id
				    instructions.push(load32(Some(
					&producer.get_bus_instance_to_field_start().to_string()
				    ))); // get position in the bus to field in memory
				    let field_no_bytes = field_no * 4;
				    instructions.push(load32(Some(&field_no_bytes.to_string()))); // get position in the field info in memory
				    if let AccessType::Qualified(_) = &indexes[idxpos] {
					instructions.push(tee_local(producer.get_io_info_tag()));
				    }
				    let field_size = producer.get_size_32_bits_in_memory() * 4;
				    instructions.push(set_constant(&field_size.to_string()));
				    instructions.push(load32(None)); // get the offset
				    instructions.push(mul32()); // mult by size of field in bytes
				    instructions.push(add32()); // add to the current offset
				    idxpos += 1;
				    if idxpos < indexes.len() {				    
					if let AccessType::Qualified(_) = &indexes[idxpos] {
					    instructions.push(get_local(producer.get_io_info_tag()));
					    instructions.push(load32(Some("4"))); // bus_id
					}
				    }
				} else {
				    assert!(false);
				}
			    }
			}
                        instructions.push(get_local(producer.get_sub_cmp_tag()));
                        instructions.push(set_constant(
                            &producer.get_signal_start_address_in_component().to_string(),
                        ));
                        instructions.push(add32());
                        instructions.push(load32(None)); //subcomponent start_of_signals: first info in the subcomponent
                        instructions.push(add32()); // we get the position of the signal (with indexes) in memory
                    }
                    _ => {
                        assert!(false);
                    }
                }
            }
        }
        if producer.needs_comments() {
            instructions.push(";; getting src".to_string());
	}
        if self.context.size > 1 {
            instructions.push(set_local(producer.get_store_aux_1_tag()));
        }
        let mut instructions_src = self.src.produce_wasm(producer);
        instructions.append(&mut instructions_src);
        if self.context.size == 1 {
            instructions.push(call("$Fr_copy"));
        } else {
            instructions.push(set_local(producer.get_store_aux_2_tag()));
            instructions.push(set_constant(&self.context.size.to_string()));
            instructions.push(set_local(producer.get_copy_counter_tag()));
            instructions.push(add_block());
            instructions.push(add_loop());
            instructions.push(get_local(producer.get_copy_counter_tag()));
            instructions.push(eqz32());
            instructions.push(br_if("1"));
            instructions.push(get_local(producer.get_store_aux_1_tag()));
            instructions.push(get_local(producer.get_store_aux_2_tag()));
            instructions.push(call("$Fr_copy"));
            instructions.push(get_local(producer.get_copy_counter_tag()));
            instructions.push(set_constant("1"));
            instructions.push(sub32());
            instructions.push(set_local(producer.get_copy_counter_tag()));
            instructions.push(get_local(producer.get_store_aux_1_tag()));
            let s = producer.get_size_32_bits_in_memory() * 4;
            instructions.push(set_constant(&s.to_string()));
            instructions.push(add32());
            instructions.push(set_local(producer.get_store_aux_1_tag()));
            instructions.push(get_local(producer.get_store_aux_2_tag()));
            instructions.push(set_constant(&s.to_string()));
            instructions.push(add32());
            instructions.push(set_local(producer.get_store_aux_2_tag()));
            instructions.push(br("0"));
            instructions.push(add_end());
            instructions.push(add_end());
        }
        match &self.dest_address_type {
            AddressType::SubcmpSignal { .. } => {
                // if subcomponent input check if run needed
		if producer.needs_comments() {
                    instructions.push(";; decrease counter".to_string()); // by self.context.size
		}
                instructions.push(get_local(producer.get_sub_cmp_tag())); // to update input signal counter
                instructions.push(get_local(producer.get_sub_cmp_tag())); // to read input signal counter
                instructions.push(load32(Some(
                    &producer.get_input_counter_address_in_component().to_string(),
                ))); //remaining inputs to be set
                instructions.push(set_constant(&self.context.size.to_string()));
                instructions.push(sub32());
                instructions.push(store32(Some(
                    &producer.get_input_counter_address_in_component().to_string(),
                ))); // update remaining inputs to be set
		if producer.needs_comments() {
                    instructions.push(";; check if run is needed".to_string());
		}
                instructions.push(get_local(producer.get_sub_cmp_tag()));
                instructions.push(load32(Some(
                    &producer.get_input_counter_address_in_component().to_string(),
                )));
                instructions.push(eqz32());
                instructions.push(add_if());
		if producer.needs_comments() {
                    instructions.push(";; run sub component".to_string());
		}
                instructions.push(get_local(producer.get_sub_cmp_tag()));
                match &self.dest {
                    LocationRule::Indexed { .. } => {
                        if let Some(name) = &my_template_header {
                            instructions.push(call(&format!("${}_run", name)));
                            instructions.push(tee_local(producer.get_merror_tag()));
                            instructions.push(add_if());
                            instructions.push(set_constant(&self.message_id.to_string()));
                            instructions.push(set_constant(&self.line.to_string()));
                            instructions.push(call("$buildBufferMessage"));
                            instructions.push(call("$printErrorMessage"));
                            instructions.push(get_local(producer.get_merror_tag()));    
                            instructions.push(add_return());
                            instructions.push(add_end());
                        } else {
                            assert!(false);
                        }
                    }
                    LocationRule::Mapped { .. } => {
                        instructions.push(get_local(producer.get_sub_cmp_tag()));
                        instructions.push(load32(None)); // get template id
                        instructions.push(call_indirect(
                            &"$runsmap".to_string(),
                            &"(type $_t_i32ri32)".to_string(),
                        ));
                        instructions.push(tee_local(producer.get_merror_tag()));
                        instructions.push(add_if());
                        instructions.push(set_constant(&self.message_id.to_string()));
                        instructions.push(set_constant(&self.line.to_string()));
                        instructions.push(call("$buildBufferMessage"));
                        instructions.push(call("$printErrorMessage"));
                        instructions.push(get_local(producer.get_merror_tag()));    
                        instructions.push(add_return());
                        instructions.push(add_end());
                    }
                }
		if producer.needs_comments() {
                    instructions.push(";; end run sub component".to_string());
		}
                instructions.push(add_end());
            }
            _ => (),
        }
        if producer.needs_comments() {
            instructions.push(";; end of store bucket".to_string());
	}
        instructions
    }
}

impl WriteC for StoreBucket {
    fn produce_c(&self, producer: &CProducer, parallel: Option<bool>) -> (Vec<String>, String) {
        use c_code_generator::*;
        let mut prologue = vec![];
	let cmp_index_ref = "cmp_index_ref".to_string();
	let aux_dest_index = "aux_dest_index".to_string();
        if let AddressType::SubcmpSignal { cmp_address, .. } = &self.dest_address_type {
            let (mut cmp_prologue, cmp_index) = cmp_address.produce_c(producer, parallel);
            prologue.append(&mut cmp_prologue);
	    prologue.push(format!("{{"));
	    prologue.push(format!("uint {} = {};",  cmp_index_ref, cmp_index));
	}
        let ((mut dest_prologue, dest_index), my_template_header) =
            if let LocationRule::Indexed { location, template_header } = &self.dest {
                (location.produce_c(producer, parallel), template_header.clone())
            } else if let LocationRule::Mapped { signal_code, indexes} = &self.dest {
        //if Mapped must be SubcmpSignal
		let mut map_prologue = vec![];
		let sub_component_pos_in_memory = format!("{}[{}]",MY_SUBCOMPONENTS,cmp_index_ref.clone());
		let mut map_access = format!("{}->{}[{}].defs[{}].offset",
					     circom_calc_wit(), template_ins_2_io_info(),
					     template_id_in_component(sub_component_pos_in_memory.clone()),
					     signal_code.to_string());
	        if indexes.len() > 0 {
	            map_prologue.push(format!("{{"));
		    //cur_def contains a pointer to the definion of the next acces.
		    //The first time it is taken from template_ins_2_io_info
		    map_prologue.push(format!("IOFieldDef *cur_def = &({}->{}[{}].defs[{}]);",
					    circom_calc_wit(), template_ins_2_io_info(),
					    template_id_in_component(sub_component_pos_in_memory.clone()),
					      signal_code.to_string()));
		    map_prologue.push(format!("uint map_accesses_aux[{}];",indexes.len().to_string()));	
		    let mut idxpos = 0;
		    while idxpos < indexes.len() {
			if let AccessType::Indexed(index_list) = &indexes[idxpos] {
			    map_prologue.push(format!("{{"));
		            map_prologue.push(format!("uint map_index_aux[{}];",index_list.len().to_string()));
			    //We first compute the number of elements as
			    //((map_index_aux[0] * length_of_dim[1]) + map_index_aux[1]) * length_of_dim[2] + ... )* length_of_dim[n-1] + map_index_aux[n-1] with
			    // map_index_aux[i] = computation of index_list[i]
		            let (mut index_code_0, mut map_index) = index_list[0].produce_c(producer, parallel);
		            map_prologue.append(&mut index_code_0);
		            map_prologue.push(format!("map_index_aux[0]={};",map_index));
		            map_index = format!("map_index_aux[0]");
		            for i in 1..index_list.len() {
				let (mut index_code, index_exp) = index_list[i].produce_c(producer, parallel);
				map_prologue.append(&mut index_code);
				map_prologue.push(format!("map_index_aux[{}]={};",i.to_string(),index_exp));
				map_index = format!("({})*cur_def->lengths[{}]+map_index_aux[{}]",
						    map_index,(i-1).to_string(),i.to_string());
		            }
		            map_prologue.push(format!("map_accesses_aux[{}] = {}", idxpos.to_string(), map_index));
			    map_prologue.push(format!("}}"));
			    // add to the access expression the computed offset in the array
			    // multiplied buy the size of the elements
			    map_access = format!("{}+map_accesses_aux[{}]*cur_def->size",
						 map_access, idxpos.to_string());
			} else if let AccessType::Qualified(_) = &indexes[idxpos] {
			    // we already have the cur_def
		            map_prologue.push(format!("map_accesses_aux[{}] = cur_def.offset", idxpos.to_string()));
			} else {
			    assert!(false);
			}
			idxpos += 1;
			if idxpos < indexes.len() {
			    if let AccessType::Qualified(field_no) = &indexes[idxpos] {
				// we get the next definition in cur_def from the bus bus_id
				map_prologue.push(format!("cur_def = &({}->{}[cur_def->busId].defs[{}]);",
							  circom_calc_wit(), bus_ins_2_field_info(),
							  field_no.to_string()));
			    }
			}
	            }
		}
                ((map_prologue, map_access),Some(template_id_in_component(sub_component_pos_in_memory.clone())))
	    } else {
		assert!(false);
                ((vec![], "".to_string()),Option::<String>::None)
	    };
	prologue.append(&mut dest_prologue);
        // Build dest
        let dest = match &self.dest_address_type {
            AddressType::Variable => {
                format!("&{}", lvar(dest_index.clone()))
            }
            AddressType::Signal => {
                format!("&{}", signal_values(dest_index.clone()))
            }
            AddressType::SubcmpSignal { .. } => {
                let sub_cmp_start = format!(
                    "{}->componentMemory[{}[{}]].signalStart",
                    CIRCOM_CALC_WIT, MY_SUBCOMPONENTS, cmp_index_ref
                );
                format!("&{}->signalValues[{} + {}]", CIRCOM_CALC_WIT, sub_cmp_start, dest_index.clone())
            }
        };
	//keep dest_index in an auxiliar if parallel and out put
	if let AddressType::Signal = &self.dest_address_type {
	    if parallel.unwrap() && self.dest_is_output {
        prologue.push(format!("{{"));
		prologue.push(format!("uint {} = {};",  aux_dest_index, dest_index.clone()));
	    }
	}
        // store src in dest
	prologue.push(format!("{{"));
	let aux_dest = "aux_dest".to_string();
	prologue.push(format!("{} {} = {};", T_P_FR_ELEMENT, aux_dest, dest));
        // Load src
	prologue.push(format!("// load src"));
    let (mut src_prologue, src) = self.src.produce_c(producer, parallel);
    prologue.append(&mut src_prologue);
	prologue.push(format!("// end load src"));	
        std::mem::drop(src_prologue);
        if self.context.size > 1 {
            let copy_arguments = vec![aux_dest, src, self.context.size.to_string()];
            prologue.push(format!("{};", build_call("Fr_copyn".to_string(), copy_arguments)));
	    if let AddressType::Signal = &self.dest_address_type {
        if parallel.unwrap() && self.dest_is_output {
		    prologue.push(format!("{{"));
		    prologue.push(format!("for (int i = 0; i < {}; i++) {{",self.context.size));
		    prologue.push(format!("{}->componentMemory[{}].mutexes[{}+i].lock();",CIRCOM_CALC_WIT,CTX_INDEX,aux_dest_index.clone()));
		    prologue.push(format!("{}->componentMemory[{}].outputIsSet[{}+i]=true;",CIRCOM_CALC_WIT,CTX_INDEX,aux_dest_index.clone()));
		    prologue.push(format!("{}->componentMemory[{}].mutexes[{}+i].unlock();",CIRCOM_CALC_WIT,CTX_INDEX,aux_dest_index.clone()));
		    prologue.push(format!("{}->componentMemory[{}].cvs[{}+i].notify_all();",CIRCOM_CALC_WIT,CTX_INDEX,aux_dest_index.clone()));
		    prologue.push(format!("}}"));
		    prologue.push(format!("}}"));
		    prologue.push(format!("}}"));
		}
	    }
        } else {
            let copy_arguments = vec![aux_dest, src];
            prologue.push(format!("{};", build_call("Fr_copy".to_string(), copy_arguments)));
	    if let AddressType::Signal = &self.dest_address_type {
		if parallel.unwrap() && self.dest_is_output {
		    prologue.push(format!("{}->componentMemory[{}].mutexes[{}].lock();",CIRCOM_CALC_WIT,CTX_INDEX,aux_dest_index.clone()));
		    prologue.push(format!("{}->componentMemory[{}].outputIsSet[{}]=true;",CIRCOM_CALC_WIT,CTX_INDEX,aux_dest_index.clone()));
		    prologue.push(format!("{}->componentMemory[{}].mutexes[{}].unlock();",CIRCOM_CALC_WIT,CTX_INDEX,aux_dest_index.clone()));
		    prologue.push(format!("{}->componentMemory[{}].cvs[{}].notify_all();",CIRCOM_CALC_WIT,CTX_INDEX,aux_dest_index.clone()));
		    prologue.push(format!("}}"));
		}
	    }
        }
	prologue.push(format!("}}"));
        match &self.dest_address_type {
            AddressType::SubcmpSignal{ uniform_parallel_value, input_information, .. } => {
                // if subcomponent input check if run needed
                let sub_cmp_counter = format!(
                    "{}->componentMemory[{}[{}]].inputCounter",
                    CIRCOM_CALC_WIT, MY_SUBCOMPONENTS, cmp_index_ref
                );
                let sub_cmp_counter_decrease = format!(
                    "{} -= {}",
                    sub_cmp_counter, self.context.size
                );
		if let InputInformation::Input{status} = input_information {
		    if let StatusInput::NoLast = status {
			// no need to run subcomponent
			prologue.push("// no need to run sub component".to_string());
			prologue.push(format!("{};", sub_cmp_counter_decrease));
			prologue.push(format!("assert({} > 0);", sub_cmp_counter));
		    } else {
			let sub_cmp_pos = format!("{}[{}]", MY_SUBCOMPONENTS, cmp_index_ref);
			let sub_cmp_call_arguments =
			    vec![sub_cmp_pos, CIRCOM_CALC_WIT.to_string()];
            // to create the call instruction we need to consider the cases of parallel/not parallel/ known only at execution
            if uniform_parallel_value.is_some(){
                // Case parallel
                let mut call_instructions = if uniform_parallel_value.unwrap(){
                    let sub_cmp_call_name = if let LocationRule::Indexed { .. } = &self.dest {
                        format!("{}_run_parallel", my_template_header.unwrap())
                    } else {
                        format!("(*{}[{}])", function_table_parallel(), my_template_header.unwrap())
                    };
                    let mut thread_call_instr = vec![];
                        
                        // parallelism
                        thread_call_instr.push(format!("{}->componentMemory[{}].sbct[{}] = std::thread({},{});",CIRCOM_CALC_WIT,CTX_INDEX,cmp_index_ref, sub_cmp_call_name, argument_list(sub_cmp_call_arguments)));
                        thread_call_instr.push(format!("std::unique_lock<std::mutex> lkt({}->numThreadMutex);",CIRCOM_CALC_WIT));
                        thread_call_instr.push(format!("{}->ntcvs.wait(lkt, [{}]() {{return {}->numThread <  {}->maxThread; }});",CIRCOM_CALC_WIT,CIRCOM_CALC_WIT,CIRCOM_CALC_WIT,CIRCOM_CALC_WIT));
                        thread_call_instr.push(format!("ctx->numThread++;"));
                    thread_call_instr

                }
                // Case not parallel
                else{
                    let sub_cmp_call_name = if let LocationRule::Indexed { .. } = &self.dest {
                        format!("{}_run", my_template_header.unwrap())
                    } else {
                        format!("(*{}[{}])", function_table(), my_template_header.unwrap())
                    };
                    vec![format!(
                        "{};",
                        build_call(sub_cmp_call_name, sub_cmp_call_arguments)
                    )]
                };
                if let StatusInput::Unknown = status {
                    let sub_cmp_counter_decrease_andcheck = format!("!({})",sub_cmp_counter_decrease);
                    let if_condition = vec![sub_cmp_counter_decrease_andcheck];
                    prologue.push("// run sub component if needed".to_string());
                    let else_instructions = vec![];
                    prologue.push(build_conditional(if_condition,call_instructions,else_instructions));
                } else {
                    prologue.push("// need to run sub component".to_string());
                    prologue.push(format!("{};", sub_cmp_counter_decrease));
                    prologue.push(format!("assert(!({}));", sub_cmp_counter));
                    prologue.append(&mut call_instructions);
                }
            }
            // Case we only know if it is parallel at execution
            else{
                prologue.push(format!(
                    "if ({}[{}]){{",
                    MY_SUBCOMPONENTS_PARALLEL, 
                    cmp_index_ref
                ));

                // case parallel
                let sub_cmp_call_name = if let LocationRule::Indexed { .. } = &self.dest {
                    format!("{}_run_parallel", my_template_header.clone().unwrap())
                } else {
                    format!("(*{}[{}])", function_table_parallel(), my_template_header.clone().unwrap())
                };
                let mut call_instructions = vec![];  
                    // parallelism
                    call_instructions.push(format!("{}->componentMemory[{}].sbct[{}] = std::thread({},{});",CIRCOM_CALC_WIT,CTX_INDEX,cmp_index_ref, sub_cmp_call_name, argument_list(sub_cmp_call_arguments.clone())));
                    call_instructions.push(format!("std::unique_lock<std::mutex> lkt({}->numThreadMutex);",CIRCOM_CALC_WIT));
                    call_instructions.push(format!("{}->ntcvs.wait(lkt, [{}]() {{return {}->numThread <  {}->maxThread; }});",CIRCOM_CALC_WIT,CIRCOM_CALC_WIT,CIRCOM_CALC_WIT,CIRCOM_CALC_WIT));
                    call_instructions.push(format!("ctx->numThread++;"));

                if let StatusInput::Unknown = status {
                    let sub_cmp_counter_decrease_andcheck = format!("!({})",sub_cmp_counter_decrease);
                    let if_condition = vec![sub_cmp_counter_decrease_andcheck];
                    prologue.push("// run sub component if needed".to_string());
                    let else_instructions = vec![];
                    prologue.push(build_conditional(if_condition,call_instructions,else_instructions));
                } else {
                    prologue.push("// need to run sub component".to_string());
                    prologue.push(format!("{};", sub_cmp_counter_decrease));
                    prologue.push(format!("assert(!({}));", sub_cmp_counter));
                    prologue.append(&mut call_instructions);
                }
                // end of case parallel

                prologue.push(format!("}} else {{"));
                
                // case not parallel
                let sub_cmp_call_name = if let LocationRule::Indexed { .. } = &self.dest {
                    format!("{}_run", my_template_header.unwrap())
                } else {
                    format!("(*{}[{}])", function_table(), my_template_header.unwrap())
                };
                let mut call_instructions = vec![format!(
                    "{};",
                    build_call(sub_cmp_call_name, sub_cmp_call_arguments)
                )];                   
                if let StatusInput::Unknown = status {
                    let sub_cmp_counter_decrease_andcheck = format!("!({})",sub_cmp_counter_decrease);
                    let if_condition = vec![sub_cmp_counter_decrease_andcheck];
                    prologue.push("// run sub component if needed".to_string());
                    let else_instructions = vec![];
                    prologue.push(build_conditional(if_condition,call_instructions,else_instructions));
                } else {
                    prologue.push("// need to run sub component".to_string());
                    prologue.push(format!("{};", sub_cmp_counter_decrease));
                    prologue.push(format!("assert(!({}));", sub_cmp_counter));
                    prologue.append(&mut call_instructions);
                }
                // end of not parallel case
                prologue.push(format!("}}"));
            }
        }
        } else {
		    assert!(false);
		}
            }
            _ => (),
        }
	if let AddressType::SubcmpSignal { .. } = &self.dest_address_type {
	    prologue.push(format!("}}"));
	}
	if let LocationRule::Mapped { indexes, .. } = &self.dest {
	    if indexes.len() > 0 {
    		prologue.push(format!("}}"));
	    }
	}

        (prologue, "".to_string())
    }
}
