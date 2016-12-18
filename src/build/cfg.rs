use build::CFG;
use mir::*;
use syntax::codemap::Span;

impl CFG {
    pub fn block_data(&self, block: BasicBlock) -> &BasicBlockData {
        &self.basic_blocks[block]
    }

    pub fn block_data_mut(&mut self, block: BasicBlock) -> &mut BasicBlockData {
        &mut self.basic_blocks[block]
    }

    pub fn start_new_block(&mut self,
                           span: Span,
                           name: Option<&'static str>,
                           live_decls: LiveDeclMap) -> BasicBlock {
        debug!("start_new_block(name={:?}, decls={:?})", name, live_decls); 
        let block = self.basic_blocks.push(BasicBlockData::new(span, name, live_decls));
        debug!("start_new_block: block={:?}", block); 

        block
    }

    pub fn push(&mut self, block: BasicBlock, statement: Statement) {
        self.block_data_mut(block).statements.push(statement);
    }

    pub fn terminate(&mut self,
                     block: BasicBlock,
                     source_info: SourceInfo,
                     kind: TerminatorKind) {
        debug!("terminate(block={:?}, kind={:?}", block, kind);

        let block_data = self.block_data_mut(block);

        assert!(block_data.terminator.is_none(),
                "terminate: block {:?} already has a terminator set: {:?}", block, block_data.terminator);

        block_data.terminator = Some(Terminator {
            source_info: source_info,
            kind: kind,
        });
    }
}
