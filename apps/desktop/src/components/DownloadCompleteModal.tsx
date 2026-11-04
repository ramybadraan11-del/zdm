//! ZDM Completion Dialog
//! Developed by Ramy Badraan

import React from 'react';
import { CheckCircle2, FolderOpen, ExternalLink } from 'lucide-react';
import { Modal } from './ui';
import { useI18n } from '../i18n';
import { CompletedPayload } from '../types';
import { formatBytes } from '../utils/format';

interface Props {
  payload: CompletedPayload | null;
  onClose: () => void;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
}

export const DownloadCompleteModal: React.FC<Props> = ({
  payload,
  onClose,
  onOpenFile,
  onOpenFolder,
}) => {
  const { t } = useI18n();

  return (
    <Modal
      open={!!payload}
      onClose={onClose}
      title={t('completedTitle')}
      icon={<CheckCircle2 size={18} />}
      width="sm"
      footer={
        <>
          <button type="button" onClick={onClose} className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold">
            {t('dismiss')}
          </button>
          {payload ? (
            <button
              type="button"
              onClick={() => onOpenFolder(payload.save_path)}
              className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold flex items-center gap-1.5"
            >
              <FolderOpen size={14} />
              {t('showFolder')}
            </button>
          ) : null}
          {payload ? (
            <button
              type="button"
              onClick={() => {
                onOpenFile(payload.save_path);
                onClose();
              }}
              className="btn-primary h-9 px-3.5 text-[12.5px] flex items-center gap-1.5"
            >
              <ExternalLink size={14} />
              {t('openNow')}
            </button>
          ) : null}
        </>
      }
    >
      {payload ? (
        <div className="text-center py-3">
          <div className="w-14 h-14 mx-auto rounded-2xl grid place-items-center bg-emerald-400/10 border border-emerald-400/25 text-emerald-300 mb-3">
            <CheckCircle2 size={26} />
          </div>
          <p className="text-[13px] font-bold text-ink break-all always-ltr" dir="ltr">
            {payload.filename}
          </p>
          <p className="text-[13.5px] text-inkMuted mt-1">
            {formatBytes(payload.total_size)} · {t('completedBody')}
          </p>
        </div>
      ) : null}
    </Modal>
  );
};
