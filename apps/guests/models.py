from django.db import models
from apps.core.models import HotelScopedModel, SoftDeleteModel

class Guest(HotelScopedModel, SoftDeleteModel):
    first_name=models.CharField(max_length=60); middle_name=models.CharField(max_length=60, blank=True); last_name=models.CharField(max_length=60); email=models.EmailField(blank=True)
    phone=models.CharField(max_length=20, blank=True)
    national_id=models.CharField(max_length=20, blank=True)
    guest_number=models.IntegerField(unique=True, null=True, blank=True)
    is_foreigner=models.BooleanField(default=False)
    tc_kimlik_no=models.CharField(max_length=11, blank=True, null=True)
    passport_number=models.CharField(max_length=50, blank=True, null=True)
    nationality=models.CharField(max_length=50, default='TR')
    id_photo = models.ImageField(upload_to='guests/id_photos/', null=True, blank=True)
    notes = models.TextField(blank=True)

    def save(self, *args, **kwargs):
        if not self.guest_number:
            last_guest = Guest.objects.order_by('-guest_number').first()
            if last_guest and last_guest.guest_number:
                self.guest_number = last_guest.guest_number + 1
            else:
                self.guest_number = 100000
        super().save(*args, **kwargs)

class GuestDocument(HotelScopedModel):
    guest=models.ForeignKey(Guest,on_delete=models.CASCADE,related_name='documents')
    doc_type=models.CharField(max_length=40); doc_number=models.CharField(max_length=80); metadata=models.JSONField(default=dict)
