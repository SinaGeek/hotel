from django.db import models
from django.utils import timezone

class TimeStampedModel(models.Model):
    created_at=models.DateTimeField(auto_now_add=True)
    updated_at=models.DateTimeField(auto_now=True)
    class Meta: abstract=True

class HotelScopedModel(TimeStampedModel):
    hotel_id=models.BigIntegerField(db_index=True)
    class Meta: abstract=True

class SoftDeleteModel(models.Model):
    deleted_at=models.DateTimeField(null=True, blank=True)
    class Meta: abstract=True
    def soft_delete(self):
        self.deleted_at=timezone.now(); self.save(update_fields=['deleted_at'])
